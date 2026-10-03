import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

function render(request) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-compact-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, `
    import { Composition, Group, Rect } from '@celesta/react';
    export default function Film() {
      return <Composition width={64} height={64} fps={30} durationInFrames={1}>
        <Rect id="plain" width={4} height={4} fill="#ffffff" />
        <Group id="group" x={3} rotation={10} anchorX={0.5} anchorY={0.5}>
          <Rect id="centred" x={1} y={2} scale={2} anchorX={0.5} anchorY={0.5} width={4} height={4} />
        </Group>
      </Composition>;
    }
  `);
  try {
    const result = spawnSync(process.execPath, [cli, entry], {
      input: `${JSON.stringify(request)}\n`,
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr);
    return JSON.parse(result.stdout.trim().split('\n')[1]).scene.layers;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('compactTransforms leaves out transform fields equal to the Rust defaults', () => {
  const [plain, group] = render({ time: { value: 0, timescale: 1 }, compactTransforms: true });
  // React's default anchor is the top-left corner, Rust's is the centre.
  assert.deepEqual(plain.transform, { anchor: { x: 0, y: 0 } });
  assert.deepEqual(group.transform, { position: { x: 3, y: 0 }, rotation: 10 });
  assert.deepEqual(group.content.layers[0].transform, {
    position: { x: 1, y: 2 },
    scale: { x: 2, y: 2 },
  });
});

test('frames requested without compactTransforms keep every transform field', () => {
  const [plain] = render({ time: { value: 0, timescale: 1 } });
  assert.deepEqual(plain.transform, {
    position: { x: 0, y: 0 },
    scale: { x: 1, y: 1 },
    rotation: 0,
    anchor: { x: 0, y: 0 },
  });
});
