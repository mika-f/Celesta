import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

test('Transition evaluates fade, slide, and scale from the local sequence frame', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-transition-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Sequence, Text, Transition } from '@celesta/react';\n` +
      `export default function Root() {\n` +
      `  return (\n` +
      `    <Composition width={320} height={240} fps={30} durationInFrames={20}>\n` +
      `      <Sequence from={5} durationInFrames={10}>\n` +
      `        <Transition type="fade" durationInFrames={5}><Text>fade</Text></Transition>\n` +
      `        <Transition type="slide" durationInFrames={5} slideFrom="right" distance={20}><Text>slide</Text></Transition>\n` +
      `        <Transition type="scale" direction="out" durationInFrames={5} scaleFrom={0.5}><Text>scale</Text></Transition>\n` +
      `      </Sequence>\n` +
      `    </Composition>\n` +
      `  );\n` +
      `}\n`,
  );

  try {
    const input = [5, 7, 14]
      .map((frame) => JSON.stringify({ time: { value: frame, timescale: 30 } }))
      .join('\n');
    const result = spawnSync(process.execPath, [cli, entry], { input: `${input}\n`, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const [, first, middle, last] = result.stdout.split(/\r?\n/).filter(Boolean).map(JSON.parse);

    const firstLayers = first.scene.layers[0].content.layers;
    assert.equal(firstLayers[0].opacity, 0);
    assert.equal(firstLayers[1].transform.position.x, 20);
    assert.equal(firstLayers[2].transform.scale.x, 1);

    const middleLayers = middle.scene.layers[0].content.layers;
    assert.equal(middleLayers[0].opacity, 0.5);
    assert.equal(middleLayers[1].transform.position.x, 10);

    const lastLayers = last.scene.layers[0].content.layers;
    assert.equal(lastLayers[0].opacity, 1);
    assert.equal(lastLayers[1].transform.position.x, 0);
    assert.equal(lastLayers[2].transform.scale.x, 0.5);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
