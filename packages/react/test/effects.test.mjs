import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));

test('effects on a group and layer change with the composition frame', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-effects-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, `
    import { Composition, Group, Rect, useCurrentFrame } from '@celesta/react';
    export default function Film() {
      const frame = useCurrentFrame();
      return <Composition width={64} height={64} fps={30} durationInFrames={2}>
        <Group id="group" blur={frame + 1} glow={{color: '#ff000080', blur: frame + 2}}>
          <Rect id="rect" width={10} height={10} fill="#ffffff"
            shadow={{color: '#00000080', blur: 3, offsetX: frame, offsetY: 2}} />
        </Group>
      </Composition>;
    }
  `);
  try {
    const result = spawnSync(process.execPath, [cli, entry], {
      input: '{"time":{"value":0,"timescale":30}}\n{"time":{"value":1,"timescale":30}}\n',
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const [, first, second] = result.stdout.split(/\r?\n/).filter(Boolean).map(JSON.parse);
    assert.equal(first.scene.layers[0].effects.blur, 1);
    assert.equal(second.scene.layers[0].effects.blur, 2);
    assert.equal(second.scene.layers[0].effects.glow.blur, 3);
    assert.equal(second.scene.layers[0].content.layers[0].effects.shadow.offsetX, 1);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
