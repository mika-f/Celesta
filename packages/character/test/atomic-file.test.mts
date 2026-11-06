import assert from 'node:assert/strict';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
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
    // One byte more than the folder's size, whatever the filesystem reports.
    await assert.rejects(write(target, 'a'.repeat(statSync(target).size + 1), { ifExists: 'same-size' }));
    assert.deepEqual(readdirSync(dir), ['taken']);
  });

  test(`with ifExists 'same-size', a folder of the data's size is still an error (${form})`, async () => {
    const target = join(dir, 'taken');
    mkdirSync(target);
    await assert.rejects(write(target, 'a'.repeat(statSync(target).size), { ifExists: 'same-size' }));
    assert.deepEqual(readdirSync(dir), ['taken']);
  });

  test(`a failed write throws and removes the temporary file (${form})`, async () => {
    await assert.rejects(write(join(dir, 'missing', 'out.wav'), 'data'));
    assert.deepEqual(readdirSync(dir), []);
  });

  // A read-only folder stops the temporary file from being written; Windows
  // ignores the mode, and root writes anyway.
  test.skipIf(process.platform === 'win32' || process.getuid?.() === 0)(
    `ifExists 'same-size' does not cover a failed write (${form})`,
    async () => {
      const folder = join(dir, 'locked');
      mkdirSync(folder);
      const target = join(folder, 'out.wav');
      writeFileSync(target, 'old!');
      chmodSync(folder, 0o555);
      try {
        // The target already has as many bytes as the data, but this write never happened.
        await assert.rejects(write(target, 'new!', { ifExists: 'same-size' }), { code: 'EACCES' });
        assert.equal(readFileSync(target, 'utf8'), 'old!');
      } finally {
        chmodSync(folder, 0o755);
      }
    },
  );
}
