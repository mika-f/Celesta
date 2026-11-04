// Exercises the built CLI (bin/ loads dist/cli.js): run `pnpm run
// build:runtime` (repository root) first.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import type { Time } from '@celesta.studio/react';
import type { FrameMessage, FrameRequest } from '../src/cli.js';
import { contentOf } from './layer-content.mjs';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

function frames(requests: FrameRequest[]): FrameMessage[] {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-static-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, `
    import { Composition, Rect, Static } from '@celesta.studio/react';
    export default function Film() {
      return <Composition width={16} height={16} fps={30} durationInFrames={3}>
        <Static><Rect width={4} height={4} fill="#ffffff" /></Static>
      </Composition>;
    }
  `);
  try {
    const result = spawnSync(process.execPath, [cli, entry], {
      input: requests.map((request) => `${JSON.stringify(request)}\n`).join(''),
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim().split('\n').slice(1).map((line) => JSON.parse(line));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('frames asked for with statics number themselves and define what Rust lacks', () => {
  const time = (value: number): Time => ({ value, timescale: 30 });
  const [first, second] = frames([
    { time: time(0), compactTransforms: true, statics: { decoded: 0 } },
    { time: time(1), compactTransforms: true, statics: { decoded: 1 } },
  ]);
  const { key } = contentOf(first.scene.layers[0], 'staticRef');
  assert.equal(first.seq, 1);
  assert.deepEqual(first.statics?.map((definition) => definition.key), [key]);
  assert.equal('droppedStatics' in first, false);
  assert.equal(second.seq, 2);
  assert.equal('statics' in second, false);
  assert.deepEqual(second.scene.layers[0].content, { type: 'staticRef', key });
});

test('frames asked for without statics have no seq and inline layers', () => {
  const [frame] = frames([{ time: { value: 0, timescale: 30 } }]);
  assert.equal('seq' in frame, false);
  assert.equal(frame.scene.layers[0].content.type, 'group');
});
