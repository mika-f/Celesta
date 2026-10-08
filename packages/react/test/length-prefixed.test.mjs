import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

/** Splits stdout into the JSON messages its u32 little-endian prefixes delimit. */
function messages(stdout) {
  const result = [];
  let offset = 0;
  while (offset < stdout.length) {
    const length = stdout.readUInt32LE(offset);
    result.push(JSON.parse(stdout.subarray(offset + 4, offset + 4 + length).toString('utf8')));
    offset += 4 + length;
  }
  assert.equal(offset, stdout.length);
  return result;
}

test('--length-prefixed frames every message with its UTF-8 byte count', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-length-prefixed-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, `
    import { Composition, Rect } from '@celesta/react';
    export default function Film() {
      return <Composition width={64} height={64} fps={30} durationInFrames={1}>
        <Rect id="四角" width={4} height={4} fill="#ffffff" />
      </Composition>;
    }
  `);
  try {
    const result = spawnSync(process.execPath, [cli, entry, '--length-prefixed'], {
      input: `${JSON.stringify({ time: { value: 0, timescale: 1 } })}\n`,
    });
    assert.equal(result.status, 0, result.stderr.toString());
    const [ready, frame] = messages(result.stdout);
    assert.equal(ready.config.width, 64);
    // A multi-byte id checks that the prefix counts bytes, not characters.
    assert.equal(frame.scene.layers[0].id, '四角');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
