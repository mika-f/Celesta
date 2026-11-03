// Integration test of the build: runs packages/cli/bin, which loads the
// built dist/ of this package and the others, so it stays on built output.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import type { SpawnSyncReturns } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import { contentOf, parseLine } from './helpers.mjs';
const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));
function renderTree(tree: string, seconds = 0) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-image-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, `import { Composition, Image, Sequence, Static } from '@celesta.studio/react';
    export default () => <Composition width={100} height={100} fps={30} durationInFrames={90}>${tree}</Composition>;`);
  const input = `{"time":{"value":${seconds * 30},"timescale":30}}\n`;
  try { return spawnSync(process.execPath, [cli, entry], { input, encoding: 'utf8' }); }
  finally { rmSync(dir, { recursive: true, force: true }); }
}
function render(props: string) {
  return renderTree(`<Image src="./logo.svg" ${props} />`);
}
function frameOf(result: SpawnSyncReturns<string>) {
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim().split(/\r?\n/).map(parseLine)[1];
}
test('image sizing reaches the scene without preloading SVG metadata', () => {
  const result = render('width={432} height={200} fit="cover"');
  assert.equal(result.status, 0, result.stderr);
  const [, frame] = result.stdout.trim().split(/\r?\n/).map(parseLine);
  assert.deepEqual(frame.scene.layers[0].content, {
    type: 'image', asset: { id: './logo.svg', location: { type: 'file', path: './logo.svg' } }, width: 432, height: 200, fit: 'cover',
    animation: { sourceTimeSeconds: 0, loop: true },
  });
});
test('an animated image plays on its sequence clock from startFrom at playbackRate', () => {
  const frame = frameOf(renderTree(`
    <Image src="./a.gif" />
    <Sequence from={30}><Image src="./a.gif" startFrom={0.25} playbackRate={2} loop={false} /></Sequence>
    <Static deps={[]}><Image src="./a.gif" /></Static>`, 2));
  const [plain, sequence, still] = frame.scene.layers;
  assert.deepEqual(contentOf(plain, 'image').animation, { sourceTimeSeconds: 2, loop: true });
  assert.deepEqual(contentOf(contentOf(sequence, 'group').layers[0], 'image').animation, { sourceTimeSeconds: 0.25 + 1 * 2, loop: false });
  // A <Static> subtree cannot depend on the time: its image draws the first frame.
  assert.equal(JSON.stringify(still).includes('animation'), false);
});
test('invalid animation props fail evaluation', () => {
  for (const props of ['startFrom={NaN}', 'playbackRate="2"', 'loop={1}']) {
    assert.match(frameOf(render(props)).error ?? '', /Image (startFrom|playbackRate|loop)/, props);
  }
});
test('invalid image dimensions and fit fail evaluation', () => {
  for (const props of ['width={0}', 'height={NaN}', 'width={-1}', 'fit="stretch"']) {
    const result = render(props);
    assert.equal(result.status, 0, result.stderr);
    const [, frame] = result.stdout.trim().split(/\r?\n/).map(parseLine);
    assert.match(frame.error ?? '', /Image (width|height|fit)/, props);
  }
});
