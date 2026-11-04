import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import type { LineDash } from '@celesta.studio/react';
import { contentOf, parseLine } from './helpers.mjs';
import type { FrameResponse } from './helpers.mjs';

// Integration test of the build: runs packages/cli/bin, which loads the built
// dist/ of this package and the others, so it stays on built output.
const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));

/**
 * Renders frames 0 to `frames - 1` of a composition whose body is `body`, as
 * the CLI's output lines: the config, then one line per frame.
 */
function render(body: string, options: { allowFailure: true }): [unknown, { error?: string }];
function render(body: string, options?: { frames?: number }): FrameResponse[];
function render(body: string, { allowFailure = false, frames = 1 }: { allowFailure?: boolean; frames?: number } = {}): unknown[] {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-dash-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Rect, useCurrentFrame } from '@celesta.studio/react'; import { Arrow, Circle, Ellipse, Line, Path, Polyline } from '@celesta.studio/shapes';\n` +
      `function Body() { const frame = useCurrentFrame(); return <>${body}</>; }\n` +
      `export default function Root() {\n` +
      `  return (\n` +
      `    <Composition width={300} height={200} fps={30} durationInFrames={${frames}}>\n` +
      `      <Body />\n` +
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

test('a dashed path carries its pattern and offset to the scene', () => {
  const [, frame] = render(
    `<Path points={[[0, 0], [100, 0]]} stroke="#FFFFFF" dash={[12, 6]} dashOffset={3} />
     <Line x1={0} y1={10} x2={100} y2={10} dash={[0, 8]} />
     <Polyline points={[[0, 20], [50, 20], [50, 70]]} dash={[5]} />
     <Path points={[[0, 0], [100, 0]]} stroke="#FFFFFF" dash={[]} dashOffset={3} />`,
  );
  const [path, dots, polyline, solid] = frame.scene.layers.map((layer) => contentOf(layer, 'path'));
  assert.deepEqual(path.lineDash, { pattern: [12, 6], offset: 3 });
  // A dotted line: zero-length dashes with the line's round caps.
  assert.deepEqual(dots.lineDash, { pattern: [0, 8] });
  assert.equal(dots.lineCap, 'round');
  // An odd pattern is repeated by the renderers, not here.
  assert.deepEqual(polyline.lineDash, { pattern: [5] });
  assert.equal(solid.lineDash, undefined);
});

test('the dash offset can change every frame', () => {
  const [, ...frames] = render(
    `<Line x1={0} y1={0} x2={100} y2={0} dash={[10, 5]} dashOffset={-frame * 2.5} />`,
    { frames: 3 },
  );
  assert.deepEqual(
    frames.map((frame) => contentOf(frame.scene.layers[0], 'path').lineDash),
    [{ pattern: [10, 5] }, { pattern: [10, 5], offset: -2.5 }, { pattern: [10, 5], offset: -5 }],
  );
});

test('drawing a dashed polyline on keeps its dashes anchored at the start', () => {
  const [, frame] = render(
    `<Polyline points={[[0, 0], [100, 0], [100, 50]]} dash={[12, 6]} dashOffset={4} progress={0.4} cap="butt" />
     <Polyline points={[[0, 0], [100, 0], [100, 50]]} dash={[12, 6]} dashOffset={4} cap="butt" />`,
  );
  const [part, whole] = frame.scene.layers.map((layer) => contentOf(layer, 'path'));
  // The part drawn starts where the whole does, with the same pattern and
  // offset, so each dash it shows is where the whole line has it.
  assert.deepEqual(part.lineDash, whole.lineDash);
  assert.deepEqual(part.commands[0], whole.commands[0]);
  assert.deepEqual(part.commands[part.commands.length - 1], { type: 'lineTo', x: 60, y: 0 });
});

/** Whether the dash pattern is on (a dash, not a gap) `s` pixels along a subpath. */
function dashOn(lineDash: LineDash, s: number): boolean {
  const pattern = lineDash.pattern.length % 2 ? [...lineDash.pattern, ...lineDash.pattern] : lineDash.pattern;
  const period = pattern.reduce((sum, n) => sum + n, 0);
  let at = ((s + (lineDash.offset ?? 0)) % period + period) % period;
  for (let i = 0; i < pattern.length; i += 1) {
    if (at < pattern[i]) {
      return i % 2 === 0;
    }
    at -= pattern[i];
  }
  return false;
}

test('trimming the start of a dashed path keeps each dash where the whole path has it', () => {
  const [, frame] = render(
    `<Path points={[[0, 0], [100, 0], [100, 50]]} stroke="#FFFFFF" dash={[12, 6]} dashOffset={4} progressStart={0.4} />
     <Path points={[[0, 0], [100, 0], [100, 50]]} stroke="#FFFFFF" dash={[12, 6]} dashOffset={4} />`,
  );
  const [part, whole] = frame.scene.layers.map((layer) => contentOf(layer, 'path'));
  // The part starts 60 px along, so its pattern starts 60 px further in.
  assert.deepEqual(part.commands[0], { type: 'moveTo', x: 60, y: 0 });
  assert.deepEqual(part.lineDash, { pattern: [12, 6], offset: 64 });
  for (let s = 60; s < 150; s += 0.5) {
    assert.equal(dashOn(part.lineDash!, s - 60), dashOn(whole.lineDash!, s), `at ${s} px`);
  }
});

test('a trimmed dashed path moves the pattern only for the subpath it starts partway along', () => {
  const twoLines = `[{ type: 'moveTo', x: 0, y: 0 }, { type: 'lineTo', x: 100, y: 0 },
    { type: 'moveTo', x: 0, y: 50 }, { type: 'lineTo', x: 100, y: 50 }]`;
  const [, frame] = render(
    `<Path commands={${twoLines}} stroke="#FFFFFF" dash={[12, 6]} progressStart={0.25} opacity={0.5} />
     <Path commands={${twoLines}} stroke="#FFFFFF" dash={[12, 6]} progressStart={0.75} />`,
  );
  const [split, single] = frame.scene.layers;
  // Each dash pattern starts over at a moveTo, so the first line (started
  // 50 px in) and the second (whole) need different offsets: two layers.
  assert.equal(split.content.type, 'group');
  assert.equal(split.opacity, 0.5);
  const [first, second] = contentOf(split, 'group').layers.map((layer) => contentOf(layer, 'path'));
  assert.deepEqual(first.commands, [{ type: 'moveTo', x: 50, y: 0 }, { type: 'lineTo', x: 100, y: 0 }]);
  assert.deepEqual(first.lineDash, { pattern: [12, 6], offset: 50 });
  assert.deepEqual(second.commands, [{ type: 'moveTo', x: 0, y: 50 }, { type: 'lineTo', x: 100, y: 50 }]);
  assert.deepEqual(second.lineDash, { pattern: [12, 6] });
  // Starting in the last subpath, one layer is enough.
  assert.deepEqual(contentOf(single, 'path').commands, [{ type: 'moveTo', x: 50, y: 50 }, { type: 'lineTo', x: 100, y: 50 }]);
  assert.deepEqual(contentOf(single, 'path').lineDash, { pattern: [12, 6], offset: 50 });
});

test('a filled path with a trimmed dashed stroke dashes only the stroke layer', () => {
  const [, frame] = render(
    `<Path points={[[0, 0], [100, 0], [100, 100], [0, 100]]} closed fill="#112233" stroke="#FFFFFF"
       dash={[10, 5]} dashOffset={2} progressStart={0.5} />`,
  );
  const [group] = frame.scene.layers;
  assert.equal(group.content.type, 'group');
  const [fill, stroke] = contentOf(group, 'group').layers.map((layer) => contentOf(layer, 'path'));
  assert.equal(fill.lineDash, undefined);
  assert.equal(fill.commands.length, 5);
  assert.deepEqual(stroke.lineDash, { pattern: [10, 5], offset: 202 });
  assert.equal(stroke.fill, undefined);
});

test('a dashed ellipse or circle runs its dashes clockwise from its rightmost point', () => {
  const [, frame] = render(
    `<Ellipse x={10} y={20} width={100} height={60} stroke="#FFFFFF" strokeWidth={4} dash={[0, 9]} dashOffset={2} cap="round" />
     <Circle x={150} y={100} anchorX={0.5} anchorY={0.5} radius={30} stroke="#FFFFFF" strokeWidth={4} dash={[6, 4]} />`,
  );
  const [ellipse, circle] = frame.scene.layers;
  assert.equal(ellipse.content.type, 'path');
  assert.deepEqual(contentOf(ellipse, 'path').lineDash, { pattern: [0, 9], offset: 2 });
  assert.equal(contentOf(ellipse, 'path').lineCap, 'round');
  // The outline starts at the right, inset by half the stroke.
  assert.deepEqual(contentOf(ellipse, 'path').commands[0], { type: 'moveTo', x: 98, y: 30 });
  // A dashed circle is a path too, rather than a rect.
  assert.equal(circle.content.type, 'path');
  assert.deepEqual(contentOf(circle, 'path').commands[0], { type: 'moveTo', x: 28, y: 0 });
  assert.deepEqual(contentOf(circle, 'path').lineDash, { pattern: [6, 4] });
});

test('a filled dashed ellipse fills its whole box beneath its dashes', () => {
  const [, frame] = render(
    `<Ellipse x={10} y={20} width={100} height={60} opacity={0.5} fill="#3366FF" stroke="#FFFFFF" strokeWidth={4} dash={[6, 4]} />`,
  );
  const [group] = frame.scene.layers;
  assert.equal(group.content.type, 'group');
  assert.deepEqual(group.transform.position, { x: 10, y: 20 });
  assert.equal(group.opacity, 0.5);
  const [fill, stroke] = contentOf(group, 'group').layers.map((layer) => contentOf(layer, 'path'));
  assert.equal(fill.stroke, undefined);
  assert.deepEqual(fill.commands[0], { type: 'moveTo', x: 100, y: 30 });
  assert.equal(stroke.fill, undefined);
  assert.deepEqual(stroke.commands[0], { type: 'moveTo', x: 98, y: 30 });
  assert.deepEqual(stroke.lineDash, { pattern: [6, 4] });
});

test('a dashed rect strokes a path from its top-left corner clockwise over a rect fill', () => {
  const [, frame] = render(
    `<Rect x={50} y={40} width={100} height={60} anchorX={0.5} anchorY={0.5} rotation={10} cornerRadius={10}
       fill="#3366FF" stroke="#FFFFFF" strokeWidth={4} dash={[8, 4]} dashOffset={1} cap="square" />
     <Rect width={20} height={10} stroke="#FFFFFF" strokeWidth={2} dash={[]} />`,
  );
  const [group, solid] = frame.scene.layers;
  assert.equal(group.content.type, 'group');
  assert.deepEqual(group.transform.position, { x: 50, y: 40 });
  assert.equal(group.transform.rotation, 10);
  const [fill, stroke] = contentOf(group, 'group').layers;
  assert.deepEqual(fill.transform.position, { x: -50, y: -30 });
  assert.deepEqual(contentOf(fill, 'rect'), { type: 'rect', width: 100, height: 60, fill: { type: 'solid', color: '#3366FF' }, cornerRadius: 10 });
  assert.equal(stroke.content.type, 'path');
  const strokePath = contentOf(stroke, 'path');
  assert.deepEqual(strokePath.stroke, { paint: { type: 'solid', color: '#FFFFFF' }, width: 4 });
  assert.deepEqual(strokePath.lineDash, { pattern: [8, 4], offset: 1 });
  assert.equal(strokePath.lineCap, 'square');
  // Inset by half the stroke: from the end of the top-left corner, whose
  // radius shrinks by as much, along the top edge.
  assert.deepEqual(strokePath.commands.slice(0, 2), [
    { type: 'moveTo', x: -40, y: -28 },
    { type: 'lineTo', x: 40, y: -28 },
  ]);
  assert.equal(strokePath.commands[strokePath.commands.length - 1].type, 'close');
  // An empty pattern leaves a plain rect.
  assert.equal(solid.content.type, 'rect');
});

test('a dashed arrow cuts its shaft into flat dashes and keeps its head whole', () => {
  const [, frame] = render(
    `<Arrow x1={0} y1={0} x2={100} y2={0} strokeWidth={4} headLength={20} headWidth={16} dash={[10, 5]} dashOffset={5} />
     <Arrow x1={0} y1={0} x2={100} y2={0} strokeWidth={4} dash={[0, 0]} />`,
  );
  const [dashed, solid] = frame.scene.layers.map((layer) => contentOf(layer, 'path'));
  assert.equal(dashed.stroke, undefined);
  assert.deepEqual(dashed.fill, { type: 'solid', color: '#FFFFFF' });
  const subpaths: [number, number][][] = [];
  for (const command of dashed.commands) {
    if (command.type === 'moveTo') subpaths.push([]);
    if (command.type !== 'close') subpaths[subpaths.length - 1].push([command.x, command.y]);
  }
  // From the tail, 5 into the pattern: a dash 0–5, then 10–20, … 70–80, then the head.
  assert.deepEqual(subpaths, [
    [[0, -2], [5, -2], [5, 2], [0, 2]],
    [[10, -2], [20, -2], [20, 2], [10, 2]],
    [[25, -2], [35, -2], [35, 2], [25, 2]],
    [[40, -2], [50, -2], [50, 2], [40, 2]],
    [[55, -2], [65, -2], [65, 2], [55, 2]],
    [[70, -2], [80, -2], [80, 2], [70, 2]],
    [[80, -8], [100, 0], [80, 8]],
  ]);
  // A pattern with no length leaves the one solid outline.
  assert.equal(solid.commands.filter((command) => command.type === 'moveTo').length, 1);
});

test('dash lengths and offsets that cannot be drawn are refused', () => {
  const cases: [string, RegExp][] = [
    [`<Line x1={0} y1={0} x2={10} y2={0} dash={[4, -1]} />`, /<Path> dash must be an array of finite numbers of at least 0/],
    [`<Line x1={0} y1={0} x2={10} y2={0} dash={[4, NaN]} />`, /<Path> dash must be an array of finite numbers of at least 0/],
    [`<Line x1={0} y1={0} x2={10} y2={0} dash={[4, 2]} dashOffset={Infinity} />`, /<Path> dashOffset must be a finite number/],
    [`<Arrow x1={0} y1={0} x2={10} y2={0} dash={[4, -1]} />`, /<Arrow> dash must be an array of finite numbers of at least 0/],
    [`<Arrow x1={0} y1={0} x2={1e9} y2={0} dash={[0.1, 0.1]} />`, /more than 1000000 dashes/],
  ];
  for (const [body, message] of cases) {
    const [, result] = render(body, { allowFailure: true });
    assert.match(result.error ?? '', message, body);
  }
});
