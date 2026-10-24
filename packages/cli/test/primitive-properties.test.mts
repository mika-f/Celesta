// The ready message's `primitiveProperties`: the primitives' property tables,
// for the editor's Properties tab. Run after `pnpm run build:runtime`: the
// tests exercise the built CLI (bin/ loads dist/cli.js).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import type { ReadyMessage } from '../src/cli.js';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

/** The ready message of an entry with `imports` that draws `body`. */
function ready(imports: string[], body = ''): ReadyMessage {
  const dir = realpathSync(mkdtempSync(join(tmpdir(), 'celesta-primitive-properties-')));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, [
    "import { Composition } from '@celesta/react';",
    ...imports,
    'export default function Film() {',
    `  return <Composition width={64} height={64} fps={30} durationInFrames={1}>${body}</Composition>;`,
    '}',
    '',
  ].join('\n'));
  try {
    const result = spawnSync(process.execPath, [cli, entry], { input: '', encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return JSON.parse(result.stdout.trim().split('\n')[0]!);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('the ready message lists the primitives\' property tables in order', () => {
  const message = ready([]);
  assert.equal(message.primitiveProperties.Rect?.groups[0]?.name, 'Transform');
});

test('a prop the props type requires is sent as required', () => {
  const size = ready([]).primitiveProperties.Rect?.groups.find((group) => group.name === 'Size')?.fields;
  assert.deepEqual(size?.find((row) => row.key === 'width')?.field, { type: 'number', required: true, min: 0 });
  assert.equal(size?.find((row) => row.key === 'cornerRadius')?.field.required, undefined);
});

test('an entry importing @celesta/shapes lists the shapes too', () => {
  const message = ready(["import { Star } from '@celesta/shapes';"], '<Star radius={8} points={5} />');
  assert.ok(message.primitiveProperties.Star, Object.keys(message.primitiveProperties).join(', '));
});
