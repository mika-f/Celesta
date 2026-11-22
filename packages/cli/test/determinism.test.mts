// The CLI reports the clock and random APIs an entry calls while rendering,
// once each, on the answer after the first call; prepare() keeps the real
// clock. Run after `pnpm run build:runtime` (repository root): the test
// exercises the built CLI (bin/ loads dist/cli.js).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import type { FrameRequest, NondeterminismReport } from '../src/cli.js';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

type Line = NondeterminismReport & { config?: unknown; scene?: unknown; error?: string };

function run(source: string, requests: FrameRequest[]): Line[] {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-determinism-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, source);
  try {
    const result = spawnSync(process.execPath, [cli, entry], {
      input: requests.map((request) => `${JSON.stringify(request)}\n`).join(''),
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim().split('\n').map((line) => JSON.parse(line));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const at = (frame: number): FrameRequest => ({ time: { value: frame, timescale: 30 } });

test('each API is reported once, on the answer after its first call', () => {
  const [ready, first, second, third] = run(`
    import { Composition, Text, useCurrentFrame } from '@celesta.studio/react';
    function Values() {
      const frame = useCurrentFrame();
      const values = [Math.random()];
      if (frame >= 2) values.push(Date.now());
      return <Text>{values.join(' ')}</Text>;
    }
    export default () => <Composition width={8} height={8} fps={30} durationInFrames={9}><Values /></Composition>;
  `, [at(0), at(2), at(3)]);
  // Mounting rendered frame 0 already.
  assert.deepEqual(ready.nondeterministic, ['Math.random()']);
  assert.equal(first.nondeterministic, undefined);
  assert.deepEqual(second.nondeterministic, ['Date.now()']);
  assert.equal(third.nondeterministic, undefined);
});

test('a failing frame still reports what it called', () => {
  const [ready, failed] = run(`
    import { Composition, useCurrentFrame } from '@celesta.studio/react';
    function Boom() {
      if (useCurrentFrame() === 1) { performance.now(); throw new Error('boom'); }
      return null;
    }
    export default () => <Composition width={8} height={8} fps={30} durationInFrames={9}><Boom /></Composition>;
  `, [at(1)]);
  assert.equal(ready.nondeterministic, undefined);
  assert.equal(failed.error, 'boom');
  assert.deepEqual(failed.nondeterministic, ['performance.now()']);
});

test('module scope and prepare() keep the real clock and are not reported', () => {
  const [ready, frame] = run(`
    import { Composition, Text } from '@celesta.studio/react';
    const loaded = Date.now();
    let prepared = 0;
    export async function prepare() { prepared = Date.now() + Math.random() * 0; }
    export default () => (
      <Composition width={8} height={8} fps={30} durationInFrames={9}>
        <Text>{String(loaded > 1e12 && prepared > 1e12)}</Text>
      </Composition>
    );
  `, [at(4)]);
  assert.equal(ready.nondeterministic, undefined);
  assert.equal(frame.nondeterministic, undefined);
  assert.match(JSON.stringify(frame.scene), /"text":"true"/);
});
