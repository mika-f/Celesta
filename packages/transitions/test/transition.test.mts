import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import type { Layer, LayerContent, Scene } from '@celesta.studio/react';
import { applyTransition } from '../src/index.js';
import type { TransitionType } from '../src/index.js';

/** A line `celesta-react-render` answers a frame request with. */
interface FrameResponse {
  scene: Scene;
}

/** The layers of a group layer. */
function childrenOf(layer: Layer | undefined): Layer[] {
  assert.equal(layer?.content.type, 'group');
  return (layer!.content as Extract<LayerContent, { type: 'group' }>).layers;
}

const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));

// Integration test of the build: runs packages/cli/bin, which loads the built
// dist/ of this package and the others, so it stays on built output.
test('Transition evaluates fade, slide, and scale from the local sequence frame', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-transition-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Sequence, Text } from '@celesta.studio/react'; import { Transition } from '@celesta.studio/transitions';\n` +
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
    const [, first, middle, last] = result.stdout.split(/\r?\n/).filter(Boolean).map((line): FrameResponse => JSON.parse(line));

    const firstLayers = childrenOf(first.scene.layers[0]);
    assert.equal(firstLayers[0].opacity, 0);
    assert.equal(firstLayers[1].transform.position.x, 20);
    assert.equal(firstLayers[2].transform.scale.x, 1);

    const middleLayers = childrenOf(middle.scene.layers[0]);
    assert.equal(middleLayers[0].opacity, 0.5);
    assert.equal(middleLayers[1].transform.position.x, 10);

    const lastLayers = childrenOf(last.scene.layers[0]);
    assert.equal(lastLayers[0].opacity, 1);
    assert.equal(lastLayers[1].transform.position.x, 0);
    assert.equal(lastLayers[2].transform.scale.x, 0.5);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('applyTransition is the effect Transition draws, on top of the group\'s own props', () => {
  const own = { opacity: 0.5, x: 10, y: 20, scale: 2, scaleX: 3, rotation: 15 };
  assert.deepEqual(applyTransition(own, { type: ['fade', 'slide', 'scale'], slideFrom: 'bottom', distance: 40, scaleFrom: 0.5 }, 0.5),
    { ...own, opacity: 0.25, x: 10, y: 40, scale: 1.5, scaleX: 2.25, scaleY: undefined });
  // At rest the props come back as written.
  assert.deepEqual(applyTransition(own, { type: ['fade', 'slide', 'scale'] }, 1), { ...own, scaleY: undefined });
  // Wrong on purpose: the check is for untyped JavaScript callers.
  assert.throws(() => applyTransition({}, { type: 'zoom' as TransitionType }, 1), /unknown <Transition> type "zoom"/);
});
