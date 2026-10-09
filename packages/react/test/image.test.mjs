import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';
const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));
function render(props) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-image-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, `import { Composition, Image } from '@celesta/react';
    export default () => <Composition width={100} height={100} fps={30} durationInFrames={1}><Image src="./logo.svg" ${props} /></Composition>;`);
  try { return spawnSync(process.execPath, [cli, entry], { input: '{"time":{"value":0,"timescale":1}}\n', encoding: 'utf8' }); }
  finally { rmSync(dir, { recursive: true, force: true }); }
}
test('image sizing reaches the scene without preloading SVG metadata', () => {
  const result = render('width={432} height={200} fit="cover"');
  assert.equal(result.status, 0, result.stderr);
  const [, frame] = result.stdout.trim().split(/\r?\n/).map(JSON.parse);
  assert.deepEqual(frame.scene.layers[0].content, { type: 'image', asset: { id: './logo.svg', location: { type: 'file', path: './logo.svg' } }, width: 432, height: 200, fit: 'cover' });
});
test('invalid image dimensions and fit fail evaluation', () => {
  for (const props of ['width={0}', 'height={NaN}', 'width={-1}', 'fit="stretch"']) {
    const result = render(props);
    assert.equal(result.status, 0, result.stderr);
    const [, frame] = result.stdout.trim().split(/\r?\n/).map(JSON.parse);
    assert.match(frame.error, /Image (width|height|fit)/, props);
  }
});
