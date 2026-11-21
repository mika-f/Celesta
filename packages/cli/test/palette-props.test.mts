// A palette picked with `--props`: palettes.property() is a `select`
// project property, so the CLI checks the name against the palettes and the
// film draws with the chosen one. Exercises the built CLI (inspect.mjs runs
// dist/cli.js): run `pnpm run build:runtime` (repository root) first.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import type { FrameMessage } from '../src/cli.js';

const inspector = fileURLToPath(new URL('../../../skills/celesta/scripts/inspect.mjs', import.meta.url));
const runtime = fileURLToPath(new URL('../dist/cli.js', import.meta.url));

const ENTRY = `
  import { Composition, Rect, Theme, alpha, defineProjectProperties, definePalettes, getProjectProperty } from '@celesta.studio/react';
  const palettes = definePalettes({
    harbor: { ground: '#0F2A3D', main: '#5FD3B5', second: '#EAF4F2', accent: '#F2B544' },
    paper: { ground: '#F4F1EA', main: '#1F4E79', second: '#111111', accent: '#FF4D1F' },
    dim: { ground: '#F4F1EA', main: '#1F4E79', second: '#999999', accent: '#FF4D1F' },
  }, { derived: { veil: alpha('second', 0.5) } });
  defineProjectProperties({ palette: palettes.property({ label: 'Palette' }) });
  export default () => <Composition width={40} height={40} fps={30} durationInFrames={1}>
    <Theme palette={palettes.get(getProjectProperty('palette'))}>
      <Rect width={40} height={40} fill="ground" />
      <Rect width={20} height={20} fill="veil" />
    </Theme>
  </Composition>;
`;

function inspect(entry: string, props?: string) {
  return spawnSync(process.execPath, [inspector, entry, '--no-native', '--runtime', runtime, '--json', '--frames', '0',
    ...(props === undefined ? [] : ['--props', props])], { encoding: 'utf8' });
}

function fills(stdout: string): string[] {
  const { frames }: { frames: FrameMessage[] } = JSON.parse(stdout);
  return frames[0]!.scene.layers.map((layer) => {
    assert.equal(layer.content.type, 'rect');
    const fill = layer.content.fill;
    assert.ok(fill && fill.type === 'solid');
    return fill.color;
  });
}

test('--props picks a palette, the default without one, and refuses names that are not palettes', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-palette-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, ENTRY);
  try {
    const byDefault = inspect(entry);
    assert.equal(byDefault.status, 0, byDefault.stderr);
    assert.deepEqual(fills(byDefault.stdout), ['#0F2A3D', '#EAF4F280']);
    // The low-contrast palette is reported on stderr, whichever palette is drawn.
    assert.match(byDefault.stderr, /palette "dim": second #999999 on ground #F4F1EA has 2\.52:1 contrast, below 4\.5:1/);

    const picked = inspect(entry, '{"palette":"paper"}');
    assert.equal(picked.status, 0, picked.stderr);
    assert.deepEqual(fills(picked.stdout), ['#F4F1EA', '#11111180']);

    const unknown = inspect(entry, '{"palette":"night"}');
    assert.notEqual(unknown.status, 0);
    assert.match(unknown.stdout + unknown.stderr, /palette[\s\S]*"harbor", "paper", "dim"/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
