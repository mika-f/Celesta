import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, test } from 'vitest';

import { writeFileAtomic, writeFileAtomicSync } from '../src/index.js';

let dir: string;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'celesta-atomic-'));
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

test('both forms write the file, replace an old one, and leave no temporary file', async () => {
  const target = join(dir, 'out.json');
  writeFileAtomicSync(target, 'old');
  await writeFileAtomic(target, new TextEncoder().encode('new'));
  assert.equal(readFileSync(target, 'utf8'), 'new');
  assert.deepEqual(readdirSync(dir), ['out.json']);
});

// A folder at the target makes the rename fail on every platform.
for (const [form, write] of [
  ['sync', async (...args: Parameters<typeof writeFileAtomicSync>) => writeFileAtomicSync(...args)],
  ['async', writeFileAtomic],
] as const) {
  test(`a failed rename throws and removes the temporary file (${form})`, async () => {
    const target = join(dir, 'taken');
    mkdirSync(target);
    await assert.rejects(write(target, 'data'));
    assert.deepEqual(readdirSync(dir), ['taken']);
  });

  test(`with ifExists 'same-size', a target of another size is still an error (${form})`, async () => {
    const target = join(dir, 'taken');
    mkdirSync(target);
    writeFileSync(join(target, 'inside'), '');
    await assert.rejects(write(target, 'a'.repeat(1000), { ifExists: 'same-size' }));
    assert.deepEqual(readdirSync(dir), ['taken']);
  });

  test(`a failed write throws and removes the temporary file (${form})`, async () => {
    await assert.rejects(write(join(dir, 'missing', 'out.wav'), 'data'));
    assert.deepEqual(readdirSync(dir), []);
  });
}
