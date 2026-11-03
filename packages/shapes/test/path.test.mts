import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import { pointOnPath } from '../src/index.js';
import type { PathCommand } from '../src/index.js';
import { commandOf, contentOf, parseLine } from './helpers.mjs';
import type { FrameResponse, OnCurveCommand } from './helpers.mjs';

type Point = [number, number];

// The tests that render() are integration tests of the build: they run
// packages/cli/bin, which loads the built dist/ of this package and the others.
const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));

/**
 * Renders frames 0 to `frames - 1` of a composition whose body is `body`, as
 * the CLI's output lines: the config, then one line per frame.
 */
function render(body: string, options: { allowFailure: true }): [unknown, { error?: string }];
function render(body: string, options?: { frames?: number }): FrameResponse[];
function render(body: string, { allowFailure = false, frames = 1 }: { allowFailure?: boolean; frames?: number } = {}): unknown[] {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-path-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Sequence, Theme } from '@celesta.studio/react'; import { Arrow, Circle, Ellipse, Line, Path, Polyline } from '@celesta.studio/shapes';\n` +
      `export default function Root() {\n` +
      `  return (\n` +
      `    <Composition width={300} height={200} fps={30} durationInFrames={${frames}}>\n` +
      `      ${body}\n` +
      `    </Composition>\n` +
      `  );\n` +
      `}\n`,
  );
  try {
    const result = spawnSync(process.execPath, [cli, entry], {
      input: Array.from({ length: frames }, (_, frame) => `{"time":{"value":${frame},"timescale":30}}\n`).join(''),
      encoding: 'utf8',
    });
    if (allowFailure && result.status !== 0) {
      return [null, { error: result.stdout + result.stderr }];
    }
    assert.equal(result.status, 0, result.stderr || result.stdout);
    return result.stdout.split(/\r?\n/).filter(Boolean).map(parseLine);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('a path reaches the scene as one layer with its commands, paint, and stroke style', () => {
  const [, frame] = render(
    `<Path x={10} y={20} opacity={0.5} fill="#112233" stroke="#FF000080" strokeWidth={3} cap="round" join="bevel" miterLimit={8}
       commands={[{ type: 'moveTo', x: 0, y: 0 }, { type: 'quadTo', x1: 5, y1: 10, x: 10, y: 0 },
         { type: 'cubicTo', x1: 12, y1: 2, x2: 14, y2: 4, x: 16, y: 0 }, { type: 'close' }]} />`,
  );
  const [path] = frame.scene.layers;
  assert.deepEqual(path.transform.position, { x: 10, y: 20 });
  assert.equal(path.opacity, 0.5);
  assert.deepEqual(contentOf(path, 'path'), {
    type: 'path',
    commands: [
      { type: 'moveTo', x: 0, y: 0 },
      { type: 'quadTo', x1: 5, y1: 10, x: 10, y: 0 },
      { type: 'cubicTo', x1: 12, y1: 2, x2: 14, y2: 4, x: 16, y: 0 },
      { type: 'close' },
    ],
    fill: { type: 'solid', color: '#112233' },
    stroke: { paint: { type: 'solid', color: '#FF000080' }, width: 3 },
    lineCap: 'round',
    lineJoin: 'bevel',
    miterLimit: 8,
  });
});

test('a path built from points closes them and omits default stroke styles', () => {
  const [, frame] = render(`<Path points={[[0, 0], [10, 0], [10, 10]]} closed stroke="#FFFFFF" />`);
  assert.deepEqual(contentOf(frame.scene.layers[0], 'path'), {
    type: 'path',
    commands: [
      { type: 'moveTo', x: 0, y: 0 },
      { type: 'lineTo', x: 10, y: 0 },
      { type: 'lineTo', x: 10, y: 10 },
      { type: 'close' },
    ],
    stroke: { paint: { type: 'solid', color: '#FFFFFF' }, width: 2 },
  });
});

test('a polyline is a single path layer, however many points it has', () => {
  const points = Array.from({ length: 500 }, (_, i) => [i, Math.sin(i / 10) * 50]);
  const [, frame] = render(`<Polyline points={${JSON.stringify(points)}} strokeWidth={1.5} opacity={0.4} />`);
  const layers = frame.scene.layers;
  assert.equal(layers.length, 1);
  assert.equal(layers[0].opacity, 0.4);
  assert.equal(layers[0].content.type, 'path');
  const content = contentOf(layers[0], 'path');
  assert.equal(content.commands.length, 500);
  assert.equal(content.lineCap, 'round');
  assert.equal(content.lineJoin, 'round');
});

test('a polyline draws the first part of its length, including a closing side', () => {
  const [, frame] = render(
    `<Polyline points={[[0, 0], [10, 0], [10, 10], [0, 10]]} closed progress={0.875} cap="butt" />`,
  );
  const { commands, lineJoin } = contentOf(frame.scene.layers[0], 'path');
  assert.equal(lineJoin, undefined);
  assert.deepEqual(commands, [
    { type: 'moveTo', x: 0, y: 0 },
    { type: 'lineTo', x: 10, y: 0 },
    { type: 'lineTo', x: 10, y: 10 },
    { type: 'lineTo', x: 0, y: 10 },
    { type: 'lineTo', x: 0, y: 5 },
  ]);
});

test('a path draws the part of its stroke between progressStart and progress, curves split exactly', () => {
  const curve: PathCommand[] = [
    { type: 'moveTo', x: 0, y: 0 },
    { type: 'quadTo', x1: 50, y1: 0, x: 100, y: 100 },
  ];
  const commands = JSON.stringify(curve);
  const [, frame] = render(
    `<Path commands={${commands}} stroke="#FFFFFF" progress={0.5} />` +
      `<Path commands={${commands}} stroke="#FFFFFF" progressStart={0.5} />` +
      `<Path commands={${commands}} stroke="#FFFFFF" progressStart={0.6} progress={0.4} />`,
  );
  const [first, second, empty] = frame.scene.layers.map((layer) => contentOf(layer, 'path').commands);
  assert.deepEqual(first.map((c) => c.type), ['moveTo', 'quadTo']);
  assert.deepEqual(second.map((c) => c.type), ['moveTo', 'quadTo']);
  // The halves meet where pointOnPath puts the tip, half the length along.
  const tip = pointOnPath(curve, 0.5);
  for (const point of [commandOf(first[1], 'quadTo'), commandOf(second[0], 'moveTo')]) {
    assert.ok(Math.abs(point.x - tip.x) < 1e-9 && Math.abs(point.y - tip.y) < 1e-9, JSON.stringify(point));
  }
  // Both halves stay on the parabola y = x² / 100.
  for (const c of [commandOf(first[1], 'quadTo'), commandOf(second[1], 'quadTo')]) {
    assert.ok(Math.abs(c.y - (c.x * c.x) / 100) < 1e-9);
  }
  assert.deepEqual(empty, []);
});

test('a trimmed closed path ends open, and its fill stays whole under the stroke', () => {
  const [, frame] = render(
    `<Path x={5} opacity={0.5} points={[[0, 0], [10, 0], [10, 10], [0, 10]]} closed fill="#112233"
       stroke="#FFFFFF" cap="round" progressStart={0.25} />`,
  );
  const [group] = frame.scene.layers;
  assert.equal(group.content.type, 'group');
  assert.deepEqual(group.transform.position, { x: 5, y: 0 });
  assert.equal(group.opacity, 0.5);
  const [fill, stroke] = contentOf(group, 'group').layers.map((layer) => contentOf(layer, 'path'));
  assert.deepEqual(fill.fill, { type: 'solid', color: '#112233' });
  assert.equal(fill.stroke, undefined);
  assert.equal(fill.commands.length, 5);
  assert.equal(stroke.fill, undefined);
  assert.equal(stroke.lineCap, 'round');
  assert.deepEqual(stroke.commands, [
    { type: 'moveTo', x: 10, y: 0 },
    { type: 'lineTo', x: 10, y: 10 },
    { type: 'lineTo', x: 0, y: 10 },
    { type: 'lineTo', x: 0, y: 0 },
  ]);
});

test('a line is a two-point path with round caps', () => {
  const [, frame] = render(`<Line x1={1} y1={2} x2={30} y2={40} stroke="#00FF00" strokeWidth={4} />`);
  assert.deepEqual(contentOf(frame.scene.layers[0], 'path'), {
    type: 'path',
    commands: [
      { type: 'moveTo', x: 1, y: 2 },
      { type: 'lineTo', x: 30, y: 40 },
    ],
    stroke: { paint: { type: 'solid', color: '#00FF00' }, width: 4 },
    lineCap: 'round',
  });
});

test('a path looks up <Theme> color names, and names neither hex nor a theme color', () => {
  const commands = `[{ type: 'moveTo', x: 0, y: 0 }, { type: 'lineTo', x: 10, y: 0 }, { type: 'lineTo', x: 10, y: 10 }]`;
  const [, frame] = render(
    `<Theme colors={{ ink: '#101010', paper: '#f4f1ea' }}>
       <Path fill="paper" stroke="ink" strokeWidth={2} commands={${commands}} />
       <Path progress={0.5} fill="paper" stroke={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 10, y: 0 },
         stops: [{ offset: 0, color: 'ink' }, { offset: 1, color: 'paper' }] }} strokeWidth={2} commands={${commands}} />
     </Theme>`,
  );
  const [whole, trimmed] = frame!.scene.layers;
  const path = contentOf(whole, 'path');
  assert.deepEqual(path.fill, { type: 'solid', color: '#f4f1ea' });
  assert.deepEqual(path.stroke?.paint, { type: 'solid', color: '#101010' });
  const [fill, stroke] = contentOf(trimmed, 'group').layers;
  assert.deepEqual(contentOf(fill, 'path').fill, { type: 'solid', color: '#f4f1ea' });
  const strokePaint = contentOf(stroke, 'path').stroke?.paint;
  assert.ok(strokePaint?.type === 'linear');
  assert.deepEqual(strokePaint.stops.map((stop) => stop.color), ['#101010', '#f4f1ea']);

  const [, failed] = render(
    `<Theme colors={{ ink: '#101010' }}><Path stroke="inc" strokeWidth={2} commands={${commands}} /></Theme>`,
    { allowFailure: true },
  );
  // The error arrives as a JSON line, its quotes escaped.
  assert.match(failed.error ?? '', /<Path> stroke \\"inc\\" is neither a #RRGGBB or #RRGGBBAA color nor a <Theme> color name \(ink\)/);
});

test('path commands and styles are validated', () => {
  const cases: [string, RegExp][] = [
    [`<Path stroke="#FFFFFF" />`, /requires `commands` or `points`/],
    [`<Path commands={[{ type: 'arcTo', x: 1, y: 1 }]} />`, /command type must be/],
    [`<Path commands={[{ type: 'lineTo', x: 1, y: NaN }]} />`, /lineTo command requires a finite `y`/],
    [`<Path points={[[0, 0], [1, 1]]} stroke="#FFFFFF" join="sharp" />`, /join must be one of/],
    [`<Path points={[[0, 0], [1, 1]]} stroke="#FFFFFF" miterLimit={0.5} />`, /miterLimit/],
    [`<Polyline points={[[0, 0], [1, 1]]} strokeWidth={0} />`, /positive `strokeWidth`/],
  ];
  for (const [body, error] of cases) {
    // Props the component checks fail the render; the rest fail the frame.
    const [, frame] = render(body, { allowFailure: true });
    assert.match(frame.error ?? '', error, body);
  }
});

/** The on-curve points of an ellipse path: its moveTo and each cubic's end. */
function onCurve(commands: PathCommand[]): Point[] {
  return commands.filter((c): c is OnCurveCommand => c.type !== 'close').map((c): Point => [c.x, c.y]);
}

function assertClose(actual: number, expected: number, message: string) {
  assert.ok(Math.abs(actual - expected) < 1e-9, `${message}: ${actual} != ${expected}`);
}

test('a circle is a rect rounded to its radius, placed and rotated about its anchor', () => {
  const gradient = `{ type: 'radial', center: { x: 40, y: 40 }, radius: 40, stops: [{ offset: 0, color: '#FFFFFF' }, { offset: 1, color: '#000000' }] }`;
  const [, frame] = render(
    `<Circle x={150} y={100} anchorX={0.5} anchorY={0.5} radius={40} rotation={30} opacity={0.5} fill={${gradient}} stroke="#FF0000" strokeWidth={6} />`,
  );
  const [circle] = frame.scene.layers;
  assert.deepEqual(circle.transform.position, { x: 150, y: 100 });
  assert.deepEqual(circle.transform.anchor, { x: 0.5, y: 0.5 });
  assert.equal(circle.transform.rotation, 30);
  assert.equal(circle.opacity, 0.5);
  // A rect's stroke is inside it and its gradients start from its top-left,
  // as an ellipse's are.
  assert.deepEqual(contentOf(circle, 'rect'), {
    type: 'rect',
    width: 80,
    height: 80,
    cornerRadius: 40,
    fill: {
      type: 'radial',
      center: { x: 40, y: 40 },
      radius: 40,
      stops: [{ offset: 0, color: '#FFFFFF' }, { offset: 1, color: '#000000' }],
    },
    stroke: { paint: { type: 'solid', color: '#FF0000' }, width: 6 },
  });
});

test('a circle stroke defaults to 2 px and is at most its radius', () => {
  const [, frame] = render(`<Circle radius={10} stroke="#FFFFFF" /><Circle radius={10} stroke="#FFFFFF" strokeWidth={50} />`);
  assert.deepEqual(frame.scene.layers.map((layer) => contentOf(layer, 'rect').stroke?.width), [2, 10]);
});

test('an ellipse fills its box from the top-left, strokes inside it, and moves gradients with it', () => {
  const gradient = `{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 200, y: 0 }, stops: [{ offset: 0, color: '#000000' }, { offset: 1, color: '#FFFFFF' }] }`;
  const [, frame] = render(
    `<Ellipse x={10} y={20} width={200} height={100} anchorX={0.25} anchorY={1} fill={${gradient}} stroke="#FF0000" strokeWidth={10} />`,
  );
  const [ellipse] = frame.scene.layers;
  assert.deepEqual(ellipse.transform.position, { x: 10, y: 20 });
  const { commands, fill, stroke } = contentOf(ellipse, 'path');
  assert.deepEqual(stroke, { paint: { type: 'solid', color: '#FF0000' }, width: 10 });
  // Box from (-50, -100) to (150, 0); the outline is inset by half the stroke.
  const points = onCurve(commands);
  assert.deepEqual(points[0], [145, -50]);
  assertClose(points[2][0], 50, 'bottom x');
  assertClose(points[2][1], -5, 'bottom y');
  assertClose(points[4][0], -45, 'left x');
  assertClose(points[6][1], -95, 'top y');
  // Eight cubic arcs, each bulging as far as the ellipse at its midpoint.
  assert.deepEqual(commands.map((c) => c.type), ['moveTo', ...Array(8).fill('cubicTo'), 'close']);
  const first = commandOf(commands[1], 'cubicTo');
  const mid = [(145 + 3 * first.x1 + 3 * first.x2 + first.x) / 8, (-50 + 3 * first.y1 + 3 * first.y2 + first.y) / 8];
  assert.ok(Math.abs(Math.hypot((mid[0] - 50) / 95, (mid[1] + 50) / 45) - 1) < 1e-5, `midpoint ${mid}`);
  assert.ok(fill?.type === 'linear');
  assert.deepEqual(fill.start, { x: -50, y: -100 });
  assert.deepEqual(fill.end, { x: 150, y: -100 });
});

test('an ellipse stroke is at most half its shorter side, and an empty ellipse draws nothing', () => {
  const [, frame] = render(`<Ellipse width={40} height={20} stroke="#FFFFFF" strokeWidth={50} />`);
  const { commands, stroke } = contentOf(frame.scene.layers[0], 'path');
  assert.equal(stroke?.width, 10);
  assert.deepEqual(onCurve(commands)[0], [35, 10]);
  for (const body of [`<Ellipse width={0} height={20} fill="#FFFFFF" />`, `<Circle radius={0} stroke="#FFFFFF" />`]) {
    const [, empty] = render(body);
    assert.deepEqual(empty.scene.layers, [], body);
  }
});

test('an arrow is one filled outline whose head tip is its end point', () => {
  const [, frame] = render(`<Arrow x={5} opacity={0.5} x1={0} y1={0} x2={100} y2={0} strokeWidth={4} />`);
  const [arrow] = frame.scene.layers;
  assert.equal(arrow.opacity, 0.5);
  assert.deepEqual(arrow.transform.position, { x: 5, y: 0 });
  assert.deepEqual(contentOf(arrow, 'path'), {
    type: 'path',
    commands: [
      { type: 'moveTo', x: 0, y: -2 },
      { type: 'lineTo', x: 84, y: -2 },
      { type: 'lineTo', x: 84, y: -8 },
      { type: 'lineTo', x: 100, y: 0 },
      { type: 'lineTo', x: 84, y: 8 },
      { type: 'lineTo', x: 84, y: 2 },
      { type: 'lineTo', x: 0, y: 2 },
      { type: 'close' },
    ],
    fill: { type: 'solid', color: '#FFFFFF' },
  });
});

test('an arrow points along its direction, with heads at the start or both ends', () => {
  const [, down] = render(`<Arrow x1={10} y1={0} x2={10} y2={50} heads="start" headLength={10} headWidth={20} stroke="#00FF00" />`);
  assert.deepEqual(onCurve(contentOf(down.scene.layers[0], 'path').commands), [
    [10, 0], [20, 10], [11, 10], [11, 50], [9, 50], [9, 10], [0, 10],
  ]);
  const [, both] = render(`<Arrow x1={0} y1={0} x2={100} y2={0} heads="both" headLength={10} headWidth={20} />`);
  assert.deepEqual(onCurve(contentOf(both.scene.layers[0], 'path').commands), [
    [0, 0], [10, -10], [10, -1], [90, -1], [90, -10], [100, 0], [90, 10], [90, 1], [10, 1], [10, 10],
  ]);
});

test('an arrow shorter than its heads shrinks them to fit, and one with no length draws nothing', () => {
  // Two 16 px heads on a 20 px arrow scale to 10 px long and 10 px wide.
  const [, short] = render(`<Arrow x1={0} y1={0} x2={20} y2={0} heads="both" strokeWidth={4} />`);
  assert.deepEqual(onCurve(contentOf(short.scene.layers[0], 'path').commands), [
    [0, 0], [10, -5], [10, -2], [10, -2], [10, -5], [20, 0], [10, 5], [10, 2], [10, 2], [10, 5],
  ]);
  // A head never gets narrower than the shaft.
  const [, tiny] = render(`<Arrow x1={0} y1={0} x2={2} y2={0} strokeWidth={4} />`);
  assert.deepEqual(onCurve(contentOf(tiny.scene.layers[0], 'path').commands), [
    [0, -2], [0, -2], [0, -2], [2, 0], [0, 2], [0, 2], [0, 2],
  ]);
  const [, empty] = render(`<Arrow x1={30} y1={40} x2={30} y2={40} />`);
  assert.deepEqual(empty.scene.layers, []);
});

test('shape dimensions are validated', () => {
  const cases: [string, RegExp][] = [
    [`<Circle radius={-1} fill="#FFFFFF" />`, /finite `radius` of at least 0/],
    [`<Circle radius={Number.MAX_VALUE} fill="#FFFFFF" />`, /diameter is finite/],
    [`<Ellipse width={NaN} height={10} fill="#FFFFFF" />`, /finite `width` of at least 0/],
    [`<Ellipse width={10} height={10} stroke="#FFFFFF" strokeWidth={0} />`, /positive `strokeWidth`/],
    [`<Circle radius={10} stroke="#FFFFFF" strokeWidth={-1} />`, /<Circle> requires a positive `strokeWidth`/],
    [`<Arrow x1={0} y1={0} x2={Infinity} y2={0} />`, /finite `x2`/],
    [`<Arrow x1={0} y1={0} x2={10} y2={0} headLength={0} />`, /positive `headLength`/],
    [`<Arrow x1={0} y1={0} x2={10} y2={0} headWidth={-3} />`, /positive `headWidth`/],
    [`<Arrow x1={0} y1={0} x2={10} y2={0} heads="none" />`, /heads must be/],
  ];
  // One CLI process renders every case, each alone in its own frame: a frame
  // that throws reports its error and the next frame renders afresh. Spawning
  // a process per case is slow enough on Windows to time the test out. Frame
  // 0 stays empty, since an error while mounting it ends the process.
  const body = cases.map(([shape], i) => `<Sequence from={${i + 1}} durationInFrames={1}>${shape}</Sequence>`).join('');
  const [, empty, ...frames] = render(body, { frames: cases.length + 1 });
  assert.deepEqual(empty.scene.layers, []);
  assert.equal(frames.length, cases.length);
  cases.forEach(([shape, error], i) => assert.match(frames[i].error ?? '', error, shape));
});
