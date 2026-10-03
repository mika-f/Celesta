import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

/** Renders frame 0 of a composition whose body is `body`, as the CLI's output lines. */
function render(body, { allowFailure = false } = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-path-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Line, Path, Polyline } from '@celesta/react';\n` +
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
