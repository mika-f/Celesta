// `openEnd` frame requests: inspect.mjs --loop evaluates frame N (one past the
// last) as if the video went on, to compare it with frame 0. Run after `pnpm
// run build:runtime` (repository root): the test exercises the built CLI (bin/
// loads dist/cli.js).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import type { FrameMessage, FrameRequest } from '../src/cli.js';
import { contentOf } from './layer-content.mjs';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

function render(requests: FrameRequest[]): FrameMessage[] {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-open-end-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, `
    import { Composition, Rect, Sequence } from '@celesta.studio/react';
    export default function Film() {
      return <Composition width={64} height={64} fps={30} durationInFrames={4}>
        <Sequence from={2}><Rect width={4} height={4} fill="#ffffff" /></Sequence>
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

test('openEnd keeps a sequence that reaches the end open past the last frame, and says so', () => {
  const [plain, open] = render([
    { time: { value: 4, timescale: 30 } },
    { time: { value: 4, timescale: 30 }, openEnd: true },
  ]);
  assert.deepEqual(plain.scene.layers, []);
  assert.equal(plain.openEnd, undefined);
  const [sequence] = open.scene.layers;
  assert.deepEqual(contentOf(sequence, 'group').layers.map((layer) => layer.content.type), ['rect']);
  // An older runtime ignores the field; the echo tells inspect.mjs it was honored.
  assert.equal(open.openEnd, true);
});
