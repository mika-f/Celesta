import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import type { Layer, LayerContent, Scene } from '@celesta.studio/react';

// Integration tests of the build: each test renders through the CLI, which
// loads @celesta.studio/layout from its dist/, so `pnpm run build:runtime` (repository
// root) must run first.
const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));

type Content<T extends LayerContent['type']> = Extract<LayerContent, { type: T }>;

/** The frame the CLI prints after its config line. */
interface RenderedFrame {
  scene: Scene;
  error?: string;
}

function frameOf(stdout: string): RenderedFrame {
  const [, frame] = stdout.split(/\r?\n/).filter(Boolean).map((line): RenderedFrame => JSON.parse(line));
  return frame;
}

/** `layer`'s content, checked to be of `type`. */
function contentOf<T extends LayerContent['type']>(layer: Layer | undefined, type: T): Content<T> {
  assert.ok(layer, `a ${type} layer`);
  assert.equal(layer.content.type, type);
  return layer.content as Content<T>;
}

/** The children of a group layer. */
const layersOf = (layer: Layer | undefined): Layer[] => contentOf(layer, 'group').layers;

test('layout components position children inside inherited bounds', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-layout-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Text } from '@celesta.studio/react'; import { Center, Fit, Grid, SafeArea, Stack } from '@celesta.studio/layout';\n` +
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
    const frame = frameOf(result.stdout);
    const safeArea = frame.scene.layers[0];
    assert.deepEqual(safeArea.transform.position, { x: 20, y: 20 });

    const [center, stack, grid, fit] = layersOf(safeArea);
    assert.deepEqual(center.transform.position, { x: 130, y: 80 });
    assert.deepEqual(layersOf(stack).map((layer) => layer.transform.position.y), [0, 15, 30]);
    assert.deepEqual(layersOf(grid).map((layer) => layer.transform.position), [
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

test('safe areas take per-edge insets and scale presets to the canvas', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-layout-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Text } from '@celesta.studio/react'; import { Center, SafeArea, safeAreaInsets } from '@celesta.studio/layout';\n` +
      `export default function Root() {\n` +
      `  return (\n` +
      `    <Composition width={1080} height={1920} fps={30} durationInFrames={1}>\n` +
      `      <SafeArea padding="vertical-social"><Center><Text>preset</Text></Center></SafeArea>\n` +
      `      <SafeArea padding={{ right: 100, bottom: 300 }}><Center><Text>edges</Text></Center></SafeArea>\n` +
      `      <Text>{JSON.stringify(safeAreaInsets('vertical-social', 720, 1280))}</Text>\n` +
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
    const frame = frameOf(result.stdout);
    const [preset, edges, scaled] = frame.scene.layers;
    // top 192, right 200, bottom 384, left 72 px on 1080×1920.
    assert.deepEqual(preset.transform.position, { x: 72, y: 192 });
    assert.deepEqual(layersOf(preset)[0].transform.position, { x: 404, y: 672 });
    assert.deepEqual(edges.transform.position, { x: 0, y: 0 });
    assert.deepEqual(layersOf(edges)[0].transform.position, { x: 490, y: 810 });
    assert.deepEqual(JSON.parse(contentOf(scaled, 'text').text), { top: 128, right: 133, bottom: 256, left: 48 });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('an unknown safe area preset names the known ones', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-layout-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition } from '@celesta.studio/react'; import { SafeArea } from '@celesta.studio/layout';\n` +
      `export default function Root() {\n` +
      `  return <Composition width={1080} height={1920} fps={30} durationInFrames={1}><SafeArea padding="shorts" /></Composition>;\n` +
      `}\n`,
  );

  try {
    const result = spawnSync(process.execPath, [cli, entry], {
      input: '{"time":{"value":0,"timescale":1}}\n',
      encoding: 'utf8',
    });
    assert.match(result.stdout, /unknown safe area preset 'shorts' \(expected 'vertical-social'\)/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('LayoutBounds sets the area nested helpers see without moving them', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-layout-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Group, Text } from '@celesta.studio/react'; import { Center, LayoutBounds, SafeArea } from '@celesta.studio/layout';\n` +
      `export default function Root() {\n` +
      `  return <Composition width={300} height={200} fps={30} durationInFrames={1}>` +
      `<Group x={40} y={30}><LayoutBounds width={100} height={60}>` +
      `<Center><Text>a</Text></Center>` +
      `<SafeArea padding={10}><Center><Text>b</Text></Center></SafeArea>` +
      `</LayoutBounds></Group>` +
      `</Composition>;\n` +
      `}\n`,
  );

  try {
    const result = spawnSync(process.execPath, [cli, entry], {
      input: '{"time":{"value":0,"timescale":1}}\n',
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const frame = frameOf(result.stdout);
    const [group] = frame.scene.layers;
    assert.deepEqual(group.transform.position, { x: 40, y: 30 });
    const [center, safeArea] = layersOf(group);
    assert.deepEqual(center.transform.position, { x: 50, y: 30 });
    assert.deepEqual(safeArea.transform.position, { x: 10, y: 10 });
    assert.deepEqual(layersOf(safeArea)[0].transform.position, { x: 40, y: 20 });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('LayoutBounds rejects a negative or non-finite size', () => {
  for (const size of ['-1', 'Infinity', 'NaN']) {
    const dir = mkdtempSync(join(tmpdir(), 'celesta-layout-'));
    const entry = join(dir, 'entry.tsx');
    writeFileSync(
      entry,
      `import { Composition } from '@celesta.studio/react'; import { LayoutBounds } from '@celesta.studio/layout';\n` +
        `export default function Root() {\n` +
        `  return <Composition width={100} height={100} fps={30} durationInFrames={1}>` +
        `<LayoutBounds width={${size}} height={10} />` +
        `</Composition>;\n` +
        `}\n`,
    );
    try {
      const result = spawnSync(process.execPath, [cli, entry], {
        input: '{"time":{"value":0,"timescale":1}}\n',
        encoding: 'utf8',
      });
      assert.match(result.stdout, /LayoutBounds width must be a finite non-negative number/, size);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }
});

test('a camera blends its view as one layer', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-layout-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Rect } from '@celesta.studio/react'; import { Camera } from '@celesta.studio/layout';\n` +
      `export default function Root() {\n` +
      `  return <Composition width={100} height={100} fps={30} durationInFrames={1}>` +
      `<Camera blendMode="softLight" opacity={0.5}><Rect width={10} height={10} fill="#808080" /></Camera>` +
      `</Composition>;\n` +
      `}\n`,
  );

  try {
    const result = spawnSync(process.execPath, [cli, entry], {
      input: '{"time":{"value":0,"timescale":1}}\n',
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const frame = frameOf(result.stdout);
    assert.equal(frame.error, undefined);
    const [camera] = frame.scene.layers;
    assert.equal(camera.blendMode, 'softLight');
    assert.equal(camera.opacity, 0.5);
    assert.equal(layersOf(camera)[0].blendMode, undefined);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
