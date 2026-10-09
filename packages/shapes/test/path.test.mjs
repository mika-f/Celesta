import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));

/** Renders frame 0 of a composition whose body is `body`, as the CLI's output lines. */
function render(body, { allowFailure = false } = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-path-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition } from '@celesta/react'; import { Arrow, Circle, Ellipse, Line, Path, Polyline } from '@celesta/shapes';\n` +
      `export default function Root() {\n` +
      `  return (\n` +
      `    <Composition width={300} height={200} fps={30} durationInFrames={1}>\n` +
      `      ${body}\n` +
      `    </Composition>\n` +
      `  );\n` +
      `}\n`,
  );
  try {
    const result = spawnSync(process.execPath, [cli, entry], {
      input: '{"time":{"value":0,"timescale":1}}\n',
      encoding: 'utf8',
    });
    if (allowFailure && result.status !== 0) {
      return [null, { error: result.stdout + result.stderr }];
    }
    assert.equal(result.status, 0, result.stderr || result.stdout);
    return result.stdout.split(/\r?\n/).filter(Boolean).map(JSON.parse);
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
  assert.deepEqual(path.content, {
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
  assert.deepEqual(frame.scene.layers[0].content, {
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
  assert.equal(layers[0].content.commands.length, 500);
  assert.equal(layers[0].content.lineCap, 'round');
  assert.equal(layers[0].content.lineJoin, 'round');
});

test('a polyline draws the first part of its length, including a closing side', () => {
  const [, frame] = render(
    `<Polyline points={[[0, 0], [10, 0], [10, 10], [0, 10]]} closed progress={0.875} cap="butt" />`,
  );
  const { commands, lineJoin } = frame.scene.layers[0].content;
  assert.equal(lineJoin, undefined);
  assert.deepEqual(commands, [
    { type: 'moveTo', x: 0, y: 0 },
    { type: 'lineTo', x: 10, y: 0 },
    { type: 'lineTo', x: 10, y: 10 },
    { type: 'lineTo', x: 0, y: 10 },
    { type: 'lineTo', x: 0, y: 5 },
  ]);
});

test('a line is a two-point path with round caps', () => {
  const [, frame] = render(`<Line x1={1} y1={2} x2={30} y2={40} stroke="#00FF00" strokeWidth={4} />`);
  assert.deepEqual(frame.scene.layers[0].content, {
    type: 'path',
    commands: [
      { type: 'moveTo', x: 1, y: 2 },
      { type: 'lineTo', x: 30, y: 40 },
    ],
    stroke: { paint: { type: 'solid', color: '#00FF00' }, width: 4 },
    lineCap: 'round',
  });
});

test('path commands and styles are validated', () => {
  const cases = [
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
function onCurve(commands) {
  return commands.filter((c) => c.type !== 'close').map((c) => [c.x, c.y]);
}

function assertClose(actual, expected, message) {
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
  assert.deepEqual(circle.content, {
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
  assert.deepEqual(frame.scene.layers.map((layer) => layer.content.stroke.width), [2, 10]);
});

test('an ellipse fills its box from the top-left, strokes inside it, and moves gradients with it', () => {
  const gradient = `{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 200, y: 0 }, stops: [{ offset: 0, color: '#000000' }, { offset: 1, color: '#FFFFFF' }] }`;
  const [, frame] = render(
    `<Ellipse x={10} y={20} width={200} height={100} anchorX={0.25} anchorY={1} fill={${gradient}} stroke="#FF0000" strokeWidth={10} />`,
  );
  const [ellipse] = frame.scene.layers;
  assert.deepEqual(ellipse.transform.position, { x: 10, y: 20 });
  const { commands, fill, stroke } = ellipse.content;
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
  const [, first] = commands;
  const mid = [(145 + 3 * first.x1 + 3 * first.x2 + first.x) / 8, (-50 + 3 * first.y1 + 3 * first.y2 + first.y) / 8];
  assert.ok(Math.abs(Math.hypot((mid[0] - 50) / 95, (mid[1] + 50) / 45) - 1) < 1e-5, `midpoint ${mid}`);
  assert.deepEqual(fill.start, { x: -50, y: -100 });
  assert.deepEqual(fill.end, { x: 150, y: -100 });
});

test('an ellipse stroke is at most half its shorter side, and an empty ellipse draws nothing', () => {
  const [, frame] = render(`<Ellipse width={40} height={20} stroke="#FFFFFF" strokeWidth={50} />`);
  const { commands, stroke } = frame.scene.layers[0].content;
  assert.equal(stroke.width, 10);
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
  assert.deepEqual(arrow.content, {
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
  assert.deepEqual(onCurve(down.scene.layers[0].content.commands), [
    [10, 0], [20, 10], [11, 10], [11, 50], [9, 50], [9, 10], [0, 10],
  ]);
  const [, both] = render(`<Arrow x1={0} y1={0} x2={100} y2={0} heads="both" headLength={10} headWidth={20} />`);
  assert.deepEqual(onCurve(both.scene.layers[0].content.commands), [
    [0, 0], [10, -10], [10, -1], [90, -1], [90, -10], [100, 0], [90, 10], [90, 1], [10, 1], [10, 10],
  ]);
});

test('an arrow shorter than its heads shrinks them to fit, and one with no length draws nothing', () => {
  // Two 16 px heads on a 20 px arrow scale to 10 px long and 10 px wide.
  const [, short] = render(`<Arrow x1={0} y1={0} x2={20} y2={0} heads="both" strokeWidth={4} />`);
  assert.deepEqual(onCurve(short.scene.layers[0].content.commands), [
    [0, 0], [10, -5], [10, -2], [10, -2], [10, -5], [20, 0], [10, 5], [10, 2], [10, 2], [10, 5],
  ]);
  // A head never gets narrower than the shaft.
  const [, tiny] = render(`<Arrow x1={0} y1={0} x2={2} y2={0} strokeWidth={4} />`);
  assert.deepEqual(onCurve(tiny.scene.layers[0].content.commands), [
    [0, -2], [0, -2], [0, -2], [2, 0], [0, 2], [0, 2], [0, 2],
  ]);
  const [, empty] = render(`<Arrow x1={30} y1={40} x2={30} y2={40} />`);
  assert.deepEqual(empty.scene.layers, []);
});

test('shape dimensions are validated', () => {
  const cases = [
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
  for (const [body, error] of cases) {
    const [, frame] = render(body, { allowFailure: true });
    assert.match(frame.error ?? '', error, body);
  }
});
