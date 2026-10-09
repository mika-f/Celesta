import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));
const psd = fileURLToPath(new URL('../../../examples/assets/lipsync-fixture.psd', import.meta.url));
const require = createRequire(import.meta.url);

test('a project outside the workspace resolves its own dependency in Node and the composition CLI', () => {
  const root = mkdtempSync(join(tmpdir(), 'celesta-project-dependencies-'));
  try {
    writeFileSync(join(root, 'package.json'), JSON.stringify({
      private: true,
      dependencies: { 'ag-psd': '^31.0.2' },
    }));
    mkdirSync(join(root, 'node_modules'));
    // Use the installed npm package without a registry request during tests.
    symlinkSync(dirname(require.resolve('ag-psd/package.json')), join(root, 'node_modules/ag-psd'), 'junction');
    writeFileSync(join(root, 'prepare-assets.mjs'), `
      import { readPsd } from 'ag-psd';
      import { readFileSync, writeFileSync } from 'node:fs';
      const { width, height } = readPsd(readFileSync(${JSON.stringify(psd)}), {
        skipLayerImageData: true, skipCompositeImageData: true, skipThumbnail: true,
      });
      writeFileSync(new URL('./prepared.json', import.meta.url), JSON.stringify({ width, height }));
    `);
    const prepare = spawnSync(process.execPath, [join(root, 'prepare-assets.mjs')], { encoding: 'utf8' });
    assert.equal(prepare.status, 0, prepare.stderr);

    const entry = join(root, 'film.tsx');
    writeFileSync(entry, `
      import { byteArrayToBase64 } from 'ag-psd';
      import { Composition, Rect, useCurrentFrame } from '@celesta/react';
      import prepared from './prepared.json';
      if (byteArrayToBase64(new Uint8Array([1, 2, 3])) !== 'AQID') throw new Error('dependency failed');
      export default function Film() {
        const frame = useCurrentFrame();
        return <Composition width={320} height={240} fps={30} durationInFrames={2}>
          <Rect id="external" x={frame} width={prepared.width} height={prepared.height} />
        </Composition>;
      }
    `);
    const result = spawnSync(process.execPath, [cli, entry], {
      input: '{"time":{"value":1,"timescale":30}}\n', encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr);
    const [metadata, frame] = result.stdout.trim().split(/\r?\n/).map(JSON.parse);
    assert.equal(metadata.config.durationInFrames, 2);
    assert.equal(frame.scene.layers[0].id, 'external');
    assert.equal(frame.scene.layers[0].transform.position.x, 1);
    assert.ok(frame.scene.layers[0].content.width > 0);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
