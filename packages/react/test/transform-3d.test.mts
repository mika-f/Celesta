// Integration test of the build: runs packages/cli/bin, which loads the
// built dist/ of this package and the others, so it stays on built output.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import type { SpawnSyncReturns } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import { parseLine } from './helpers.mjs';

const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));

function render(body: string) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-transform-3d-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Group, Rect } from '@celesta.studio/react';\n` +
      `export default function Root() {\n` +
      `  return (\n` +
      `    <Composition width={100} height={100} fps={30} durationInFrames={1}>\n` +
      `${body}\n` +
      `    </Composition>\n` +
      `  );\n` +
      `}\n`,
  );
  try {
    return spawnSync(process.execPath, [cli, entry], {
      input: '{"time":{"value":0,"timescale":1},"compactTransforms":true}\n',
      encoding: 'utf8',
    });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function frameOf(result: SpawnSyncReturns<string>) {
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const [, frame] = result.stdout.split(/\r?\n/).filter(Boolean).map(parseLine);
  return frame;
}

const NEW_KEYS = ['origin', 'skew', 'rotationX', 'rotationY', 'perspective', 'backfaceVisibility'];

test('skew, 3D rotation, perspective, and backfaceVisibility serialize into transform', () => {
  const frame = frameOf(
    render(
      `      <Rect id="a" width={10} height={10} skewX={10} rotationY={30} perspective={800} backfaceVisibility="hidden" />`,
    ),
  );
  const { transform } = frame.scene.layers[0];
  assert.deepEqual(transform.skew, { x: 10, y: 0 });
  assert.equal('rotationX' in transform, false);
  assert.equal(transform.rotationY, 30);
  assert.equal(transform.perspective, 800);
  assert.equal(transform.backfaceVisibility, 'hidden');
});

test('a Group takes rotationX and leaves the other new fields out', () => {
  const frame = frameOf(
    render(`      <Group id="g" rotationX={20}><Rect width={10} height={10} /></Group>`),
  );
  const { transform } = frame.scene.layers[0];
  assert.equal(transform.rotationX, 20);
  for (const key of ['skew', 'perspective', 'backfaceVisibility']) {
    assert.equal(key in transform, false, key);
  }
});

test('a Group takes originX and originY as its origin', () => {
  const frame = frameOf(
    render(
      `      <Group id="g" originX={50} originY={25} rotationY={40} perspective={400}><Rect width={10} height={10} /></Group>`,
    ),
  );
  const { transform } = frame.scene.layers[0];
  assert.deepEqual(transform.origin, { x: 50, y: 25 });
  assert.equal(transform.rotationY, 40);
});

test('a perspective that is not finite and positive is omitted', () => {
  for (const value of [0, -5, NaN]) {
    const frame = frameOf(render(`      <Rect width={10} height={10} perspective={${value}} />`));
    assert.equal('perspective' in frame.scene.layers[0].transform, false, String(value));
  }
});

test('a plain layer carries none of the new fields', () => {
  const frame = frameOf(render(`      <Rect width={10} height={10} />`));
  const { transform } = frame.scene.layers[0];
  for (const key of NEW_KEYS) {
    assert.equal(key in transform, false, key);
  }
});

test('an unknown backfaceVisibility is an error', () => {
  const frame = frameOf(render(`      <Rect width={10} height={10} backfaceVisibility="sideways" />`));
  assert.match(frame.error ?? '', /unknown backfaceVisibility "sideways"; expected one of visible, hidden/);
});
