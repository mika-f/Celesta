import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

test('anchorY="baseline" anchors Text on its baseline', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-text-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Text } from '@celesta/react';\n` +
      `export default function Root() {\n` +
      `  return (\n` +
      `    <Composition width={100} height={100} fps={30} durationInFrames={1}>\n` +
      `      <Text id="baseline" y={50} anchorX={0.5} anchorY="baseline">{' = '}</Text>\n` +
      `      <Text id="numeric" y={50} anchorY={0.5}>a</Text>\n` +
      `    </Composition>\n` +
      `  );\n` +
      `}\n`,
  );

  try {
    const result = spawnSync(process.execPath, [cli, entry], {
      input: '{"time":{"value":0,"timescale":1}}\n',
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const [, frame] = result.stdout.split(/\r?\n/).filter(Boolean).map(JSON.parse);
    const [baseline, numeric] = frame.scene.layers;
    assert.equal(baseline.content.text, ' = ');
    assert.equal(baseline.content.baselineAnchor, true);
    assert.equal(baseline.transform.anchor.x, 0.5);
    assert.equal(numeric.content.baselineAnchor, undefined);
    assert.equal(numeric.transform.anchor.y, 0.5);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
