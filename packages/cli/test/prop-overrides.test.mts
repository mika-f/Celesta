// Frame requests with `propOverrides`: the editor's drag preview. Run after
// `pnpm run build:runtime`: the tests exercise the built CLI (bin/ loads
// dist/cli.js).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import type { Layer } from '@celesta/react';
import type { FrameMessage, Request } from '../src/cli.js';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

const time = { value: 0, timescale: 30 };

function run(requests: (dir: string) => Request[]) {
  const dir = realpathSync(mkdtempSync(join(tmpdir(), 'celesta-prop-overrides-')));
  const entry = join(dir, 'entry.tsx');
  const label = join(dir, 'label.tsx');
  writeFileSync(label, [
    "import { Rect } from '@celesta/react';",
    'export function Label({ x }: { x: number }) {',
    '  return <><Rect id="box" x={x} width={4} height={4} /><Rect id="shadow" x={x} width={4} height={4} /></>;',
    '}',
    '',
  ].join('\n'));
  // `<Label>` is on line 6, column 9 of entry.tsx.
  writeFileSync(entry, [
    "import { Composition } from '@celesta/react';",
    "import { Label } from './label';",
    'export default function Film() {',
    '  return (',
    '    <Composition width={64} height={64} fps={30} durationInFrames={3}>',
    '        <Label x={10} />',
    '    </Composition>',
    '  );',
    '}',
    '',
  ].join('\n'));
  try {
    const result = spawnSync(process.execPath, [cli, entry, '--source-locations'], {
      input: requests(dir).map((request) => `${JSON.stringify(request)}\n`).join(''),
      encoding: 'utf8',
    });
    const responses: FrameMessage[] = result.stdout.trim().split('\n').slice(1).map((line) => JSON.parse(line));
    return { result, responses };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const xOf = (frame: FrameMessage, id: string) =>
  frame.scene.layers.find((layer: Layer) => layer.id === id)?.transform.position.x;

test('a frame request with propOverrides draws the overridden values', () => {
  const { result, responses } = run((dir) => [
    { time },
    { time, propOverrides: [{ file: join(dir, 'entry.tsx'), line: 6, column: 9, prop: 'x', value: 30 }] },
    { time, propOverrides: [] },
  ]);
  assert.equal(result.status, 0, result.stderr);
  const [before, dragged, after] = responses;
  assert.deepEqual([xOf(before, 'box'), xOf(before, 'shadow')], [10, 10], JSON.stringify(before));
  assert.deepEqual([xOf(dragged, 'box'), xOf(dragged, 'shadow')], [30, 30], JSON.stringify(dragged));
  assert.deepEqual([xOf(after, 'box'), xOf(after, 'shadow')], [10, 10], JSON.stringify(after));
});
