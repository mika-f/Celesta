import assert from 'node:assert/strict';
import { closeSync, mkdtempSync, openSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, test } from 'vitest';

import { ProtocolLines, requestQueue } from '../src/channel.js';

const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
});

/** Lines read from a file holding `contents`, as they would be from stdin. */
function linesOf(contents: string | Buffer): ProtocolLines {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-channel-'));
  const file = join(dir, 'input');
  writeFileSync(file, contents);
  const fd = openSync(file, 'r');
  cleanups.push(() => {
    closeSync(fd);
    rmSync(dir, { recursive: true, force: true });
  });
  return new ProtocolLines(fd);
}

test('lines come back in order, from async and sync reads alike', async () => {
  const lines = linesOf('one\ntwo\n\nthree\nfour');
  assert.deepEqual(await lines.next(), { done: false, value: 'one' });
  assert.deepEqual(lines.nextSync(), { done: false, value: 'two' });
  assert.deepEqual(await lines.next(), { done: false, value: '' });
  assert.deepEqual(lines.nextSync(), { done: false, value: 'three' });
  // The last line may end without a newline.
  assert.deepEqual(await lines.next(), { done: false, value: 'four' });
  assert.deepEqual(await lines.next(), { done: true, value: undefined });
  assert.deepEqual(lines.nextSync(), { done: true, value: undefined });
});

test('a line longer than many reads, and the lines around it, come back whole', async () => {
  const long = JSON.stringify({ project: 'x'.repeat(1024 * 1024 + 7) });
  const lines = linesOf(`before\n${long}\nafter\n`);
  assert.equal((await lines.next()).value, 'before');
  assert.equal((await lines.next()).value, long);
  assert.equal(lines.nextSync().value, 'after');
  assert.equal((await lines.next()).done, true);
});

test('a multi-byte character split across reads decodes intact', async () => {
  // 64 KiB reads: put a three-byte character across the first boundary.
  const head = 'a'.repeat(64 * 1024 - 1);
  const lines = linesOf(`${head}字\n`);
  assert.equal((await lines.next()).value, `${head}字`);
});

test('an empty input has no lines', async () => {
  assert.deepEqual(await linesOf('').next(), { done: true, value: undefined });
});

test('queued requests run one at a time, in order, on every function the queue wraps', async () => {
  const serialized = requestQueue();
  const log: string[] = [];
  let release!: () => void;
  const gate = new Promise<void>((resolve) => (release = resolve));
  const slow = serialized(async (name: string) => {
    log.push(`start ${name}`);
    await gate;
    log.push(`end ${name}`);
    return name;
  });
  const fast = serialized(async (n: number) => {
    log.push(`run ${n}`);
    return n * 2;
  });
  const first = slow('a');
  const second = fast(21);
  await Promise.resolve();
  assert.deepEqual(log, ['start a']);
  release();
  assert.equal(await first, 'a');
  assert.equal(await second, 42);
  assert.deepEqual(log, ['start a', 'end a', 'run 21']);
});

test('a failed request rejects to its caller and the queue goes on', async () => {
  const serialized = requestQueue();
  const failing = serialized(async () => {
    throw new Error('probe failed');
  });
  const next = serialized(async () => 'ok');
  const failed = failing();
  await assert.rejects(failed, /probe failed/);
  assert.equal(await next(), 'ok');
});
