import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

test('layout components position children inside inherited bounds', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-layout-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Center, Composition, Fit, Grid, SafeArea, Stack, Text } from '@celesta/react';\n` +
      `export default function Root() {\n` +
      `  return (\n` +
      `    <Composition width={300} height={200} fps={30} durationInFrames={1}>\n` +
      `      <SafeArea padding={20}>\n` +
      `        <Center><Text>center</Text></Center>\n` +
      `        <Stack spacing={15}><Text>a</Text><Text>b</Text><Text>c</Text></Stack>\n` +
      `        <Grid columns={2} columnWidth={50} rowHeight={40} columnGap={10} rowGap={5}><Text>a</Text><Text>b</Text><Text>c</Text></Grid>\n` +
      `        <Fit sourceWidth={100} sourceHeight={100}><Text>fit</Text></Fit>\n` +
      `      </SafeArea>\n` +
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
    const safeArea = frame.scene.layers[0];
    assert.deepEqual(safeArea.transform.position, { x: 20, y: 20 });

    const [center, stack, grid, fit] = safeArea.content.layers;
    assert.deepEqual(center.transform.position, { x: 130, y: 80 });
    assert.deepEqual(stack.content.layers.map((layer) => layer.transform.position.y), [0, 15, 30]);
    assert.deepEqual(grid.content.layers.map((layer) => layer.transform.position), [
      { x: 0, y: 0 },
      { x: 60, y: 0 },
      { x: 0, y: 45 },
    ]);
    assert.deepEqual(fit.transform.position, { x: 50, y: 0 });
    assert.deepEqual(fit.transform.scale, { x: 1.6, y: 1.6 });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
