import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

test('reordering keyed children moves them instead of duplicating them', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-reconciler-'));
  const entry = join(dir, 'entry.tsx');
  // Rotates the keyed rects by one position every frame, like a depth sort
  // does, in a group and at the composition root.
  writeFileSync(
    entry,
    `import { Composition, Group, Rect, useCurrentFrame } from '@celesta/react';\n` +
      `function Rects({ prefix }: { prefix: string }) {\n` +
      `  const frame = useCurrentFrame();\n` +
      `  const ids = [0, 1, 2, 3].map((index) => (index + frame) % 4);\n` +
      `  return <>{ids.map((id) => <Rect key={id} id={prefix + id} width={id + 1} height={1} />)}</>;\n` +
      `}\n` +
      `export default function Root() {\n` +
      `  return (\n` +
      `    <Composition width={10} height={10} fps={30} durationInFrames={8}>\n` +
      `      <Group id="group"><Rects prefix="g" /></Group>\n` +
      `      <Rects prefix="r" />\n` +
      `    </Composition>\n` +
      `  );\n` +
      `}\n`,
  );

  try {
    const frames = [0, 1, 2, 3, 4, 5];
    const result = spawnSync(process.execPath, [cli, entry], {
      input: frames.map((frame) => JSON.stringify({ time: { value: frame, timescale: 30 } })).join('\n') + '\n',
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const scenes = result.stdout
      .split(/\r?\n/)
      .filter(Boolean)
      .map(JSON.parse)
      .slice(1)
      .map((response) => response.scene);
    assert.equal(scenes.length, frames.length);
    for (const [frame, scene] of scenes.entries()) {
      const expected = [0, 1, 2, 3].map((index) => (index + frame) % 4);
      const [group, ...rects] = scene.layers;
      assert.deepEqual(
        group.content.layers.map((layer) => layer.id),
        expected.map((id) => `g${id}`),
        `group children at frame ${frame}`,
      );
      assert.deepEqual(
        rects.map((layer) => layer.id),
        expected.map((id) => `r${id}`),
        `root children at frame ${frame}`,
      );
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
