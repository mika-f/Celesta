import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

function render(body) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-blend-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Group, Rect, Text } from '@celesta/react';\n` +
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
  const [, frame] = result.stdout.split(/\r?\n/).filter(Boolean).map(JSON.parse);
  const [plain, hud] = frame.scene.layers;
  assert.equal('blendMode' in plain, false);
  assert.equal(hud.blendMode, 'difference');
  assert.equal(hud.opacity, 0.75);
  const [label, grain] = hud.content.layers;
  assert.equal(label.blendMode, 'screen');
  assert.equal(grain.blendMode, 'overlay');
});

test('an unknown blendMode is an error', () => {
  const result = render(`      <Rect width={10} height={10} fill="#ffffff" blendMode="hue" />`);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const [, frame] = result.stdout.split(/\r?\n/).filter(Boolean).map(JSON.parse);
  assert.match(frame.error, /unknown blendMode "hue"/);
});
