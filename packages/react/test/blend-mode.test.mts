// Integration test of the build: runs packages/cli/bin, which loads the
// built dist/ of this package and the others, so it stays on built output.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import { childrenOf, parseLine } from './helpers.mjs';

const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));

function render(body: string) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-blend-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Group, Rect, Text } from '@celesta.studio/react';\n` +
      `export default function Root() {\n` +
      `  return (\n` +
      `    <Composition width={100} height={100} fps={30} durationInFrames={1}>\n` +
      `${body}\n` +
      `    </Composition>\n` +
      `  );\n` +
      `}\n`,
  );
  try {
    return spawnSync(process.execPath, [cli, entry], {
      input: '{"time":{"value":0,"timescale":1}}\n',
      encoding: 'utf8',
    });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('blendMode is set on layers and groups, and omitted when normal', () => {
  const result = render(
    `      <Rect id="plain" width={10} height={10} fill="#ffffff" blendMode="normal" />\n` +
      `      <Group id="hud" blendMode="difference" opacity={0.75}>\n` +
      `        <Text id="label" blendMode="screen">HUD</Text>\n` +
      `        <Rect id="grain" width={10} height={10} fill="#808080" blendMode="overlay" />\n` +
      `      </Group>`,
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const [, frame] = result.stdout.split(/\r?\n/).filter(Boolean).map(parseLine);
  const [plain, hud] = frame.scene.layers;
  assert.equal('blendMode' in plain, false);
  assert.equal(hud.blendMode, 'difference');
  assert.equal(hud.opacity, 0.75);
  const [label, grain] = childrenOf(hud);
  assert.equal(label.blendMode, 'screen');
  assert.equal(grain.blendMode, 'overlay');
});

test('every W3C blend mode is accepted, multi-word ones in camelCase', () => {
  const modes = [
    'darken', 'lighten', 'colorDodge', 'colorBurn', 'hardLight', 'softLight', 'exclusion',
    'hue', 'saturation', 'color', 'luminosity',
  ];
  const result = render(
    modes.map((mode) => `      <Rect width={10} height={10} fill="#808080" blendMode="${mode}" />`).join('\n') +
      `\n      <Group blendMode="luminosity"><Rect width={10} height={10} fill="#808080" /></Group>`,
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const [, frame] = result.stdout.split(/\r?\n/).filter(Boolean).map(parseLine);
  assert.equal(frame.error, undefined);
  assert.deepEqual(frame.scene.layers.map((layer) => layer.blendMode), [...modes, 'luminosity']);
});

test('an unknown blendMode is an error', () => {
  // CSS spells it `color-dodge`; Celesta's names are camelCase.
  const result = render(`      <Rect width={10} height={10} fill="#ffffff" blendMode="color-dodge" />`);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const [, frame] = result.stdout.split(/\r?\n/).filter(Boolean).map(parseLine);
  assert.match(frame.error ?? '', /unknown blendMode "color-dodge"; expected one of .*colorDodge/);
});
