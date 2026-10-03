// Portrait blinking: the pure `blinkPhase()` schedule, and the layers or
// images a <CharacterView> renders on blinking and open frames. Run after
// `pnpm run build`.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { blinkPhase } from '../dist/index.js';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));
const psd = fileURLToPath(new URL('../../../examples/assets/lipsync-fixture.psd', import.meta.url));
const FPS = 30;

/** The first frame at or after `from` where `blinkPhase` is `phase`. */
function firstFrame(phase, timing, from = 0) {
  for (let frame = from; frame < from + 100_000; frame += 1) {
    if (blinkPhase(frame, FPS, timing) === phase) return frame;
  }
  throw new Error(`no ${phase} frame`);
}

/**
 * Renders `frames` of a composition holding the character "Hana" with
 * `portrait` and a view of it with `view` props, optionally inside `wrap`
 * (`[open, close]` JSX). Returns each frame's portrait: the `psd` layer
 * content, or the list of image paths for an image portrait.
 */
function render({ portrait, view = '', wrap = ['', ''], frames }) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-blink-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import * as React from 'react';\n` +
      `import { Assets, Character, CharacterView, Composition, Sequence } from '@celesta/react';\n` +
      `const hana = React.createRef();\n` +
      `export default function Root() {\n` +
      `  return <Composition width={300} height={200} fps={${FPS}} durationInFrames={100000}>\n` +
      `    <Assets><Character ref={hana} name="Hana" portrait={{ ${portrait} }} /></Assets>\n` +
      `    ${wrap[0]}<CharacterView character={hana} ${view} />${wrap[1]}\n` +
      `  </Composition>;\n` +
      `}\n`,
  );
  try {
    const input = frames.map((frame) => JSON.stringify({ time: { value: frame, timescale: FPS } })).join('\n');
    const result = spawnSync(process.execPath, [cli, entry], { input: `${input}\n`, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const [, ...responses] = result.stdout.split(/\r?\n/).filter(Boolean).map(JSON.parse);
    return responses.map((response) => {
      assert.equal(response.error, undefined, response.error);
      const find = (layers) => {
        for (const layer of layers) {
          if (layer.content.type === 'psd') return layer.content;
          if (layer.content.type === 'group') {
            const images = layer.content.layers.filter((child) => child.content.type === 'image');
            if (images.length) return images.map((child) => child.content.asset.location.path);
            const found = find(layer.content.layers);
            if (found) return found;
          }
        }
        return undefined;
      };
      return find(response.scene.layers);
    });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('blinkPhase is a pure function of the frame and the seed', () => {
  const run = (seed) => Array.from({ length: 3000 }, (_, frame) => blinkPhase(frame, FPS, { seed }));
  assert.deepEqual(run('Hana'), run('Hana'));
  assert.deepEqual(run(7), run(7));
  assert.notDeepEqual(run('Hana'), run('Mira'));
  // Rendering frames out of order gives the same answer.
  assert.equal(blinkPhase(1234, FPS, { seed: 3 }), run(3)[1234]);
});

test('blinks come about every interval, irregularly, and stay shut for duration', () => {
  const starts = [];
  const lengths = [];
  let run = 0;
  for (let frame = 0; frame < 30 * 600; frame += 1) {
    const closed = blinkPhase(frame, FPS, { seed: 'Hana' }) === 'closed';
    if (closed && run === 0) starts.push(frame);
    if (!closed && run > 0) lengths.push(run);
    run = closed ? run + 1 : 0;
  }
  const gaps = starts.slice(1).map((start, i) => (start - starts[i]) / FPS);
  const mean = gaps.reduce((a, b) => a + b, 0) / gaps.length;
  assert.ok(mean > 3.5 && mean < 4.5, `mean gap ${mean}s`);
  assert.ok(Math.min(...gaps) >= 2 && Math.max(...gaps) <= 6, `gaps ${Math.min(...gaps)}–${Math.max(...gaps)}s`);
  assert.ok(new Set(gaps).size > 10, 'gaps vary');
  assert.deepEqual(new Set(lengths), new Set([3])); // 0.1 s at 30 fps
  // The first blink is not on the first frame.
  assert.equal(blinkPhase(0, FPS, { seed: 'Hana' }), 'open');
});

test('blink timing options change the rhythm', () => {
  const count = (timing) => {
    let closed = 0;
    for (let frame = 0; frame < 30 * 600; frame += 1) {
      if (blinkPhase(frame, FPS, timing) === 'closed') closed += 1;
    }
    return closed;
  };
  assert.equal(count({ seed: 1 }), 150 * 3);
  assert.equal(count({ seed: 1, interval: 2 }), 300 * 3);
  assert.equal(count({ seed: 1, duration: 0.2 }), 150 * 6);
  // A duration shorter than a frame still shuts the eyes for one frame.
  assert.equal(count({ seed: 1, duration: 0.001 }), 150);
});

test('half-shut frames bracket every blink', () => {
  const shut = firstFrame('closed', { seed: 2 });
  assert.equal(blinkPhase(shut - 1, FPS, { seed: 2 }), 'half');
  assert.equal(blinkPhase(shut + 3, FPS, { seed: 2 }), 'half');
  assert.equal(blinkPhase(shut - 2, FPS, { seed: 2 }), 'open');
  assert.equal(blinkPhase(shut + 4, FPS, { seed: 2 }), 'open');
});

test('blinkPhase rejects bad timing', () => {
  assert.throws(() => blinkPhase(0, FPS, { interval: 0 }), /interval/);
  assert.throws(() => blinkPhase(0, FPS, { duration: -1 }), /duration/);
  assert.throws(() => blinkPhase(0, 0), /fps/);
});

const PSD = `type: 'psd', src: ${JSON.stringify(psd)}, layers: ['body'], defaultExpression: 'calm',
  blink: { open: 'eyes/open', closed: 'eyes/closed', half: 'eyes/half' },
  expressions: {
    calm: ['face/calm'],
    angry: { layers: ['face/angry'], blink: { open: ['face/angry/eyes/l', 'face/angry/eyes/r'], closed: 'face/angry/eyes/shut' } },
    sleepy: { layers: ['face/sleepy'], blink: false },
  },
  lipSync: { a: 'mouth/a', i: 'mouth/i', u: 'mouth/u', e: 'mouth/e', o: 'mouth/o', closed: 'mouth/n' }`;

test('a PSD portrait swaps its eye layers while it blinks', () => {
  const shut = firstFrame('closed', { seed: 'Hana' });
  const [open, half, closed] = render({ portrait: PSD, frames: [0, shut - 1, shut] });
  assert.deepEqual(open.visibleLayers, ['body', 'face/calm']);
  assert.deepEqual(open.enabledLayers, ['eyes/open']);
  assert.deepEqual(open.disabledLayers, ['eyes/closed', 'eyes/half']);
  assert.deepEqual(half.enabledLayers, ['eyes/half']);
  assert.deepEqual(half.disabledLayers, ['eyes/open', 'eyes/closed']);
  assert.deepEqual(closed.enabledLayers, ['eyes/closed']);
  assert.deepEqual(closed.disabledLayers, ['eyes/open', 'eyes/half']);
});

test('a PSD portrait blinks while it lip-syncs', () => {
  const shut = firstFrame('closed', { seed: 'Hana' });
  const [closed] = render({ portrait: PSD, view: 'mouth="a"', frames: [shut] });
  assert.deepEqual(closed.enabledLayers, ['mouth/a', 'eyes/closed']);
  assert.ok(closed.disabledLayers.includes('mouth/i'));
  assert.ok(closed.disabledLayers.includes('eyes/open'));
});

test('a PSD expression blinks with its own eyes, or not at all', () => {
  const shut = firstFrame('closed', { seed: 'Hana' });
  const [open, closed] = render({ portrait: PSD, view: 'expression="angry"', frames: [0, shut] });
  assert.deepEqual(open.enabledLayers, ['face/angry/eyes/l', 'face/angry/eyes/r']);
  assert.deepEqual(open.disabledLayers, ['face/angry/eyes/shut']);
  // No half-shut layer of its own: the portrait's is not borrowed.
  assert.deepEqual(closed.enabledLayers, ['face/angry/eyes/shut']);
  assert.deepEqual(closed.disabledLayers, ['face/angry/eyes/l', 'face/angry/eyes/r']);
  const [sleepy] = render({ portrait: PSD, view: 'expression="sleepy"', frames: [shut] });
  assert.equal(sleepy.enabledLayers, undefined);
  assert.equal(sleepy.disabledLayers, undefined);
});

test('blink={false} holds the eyes open', () => {
  const shut = firstFrame('closed', { seed: 'Hana' });
  const [held] = render({ portrait: PSD, view: 'blink={false}', frames: [shut] });
  assert.deepEqual(held.enabledLayers, ['eyes/open']);
  assert.deepEqual(held.disabledLayers, ['eyes/closed', 'eyes/half']);
});

test('the seed and timing can be set on the portrait, an expression, or the view', () => {
  const shut = firstFrame('closed', { seed: 'other', interval: 2 });
  const blinking = (frame, portrait, view = '') => render({ portrait, view, frames: [frame] })[0].enabledLayers.includes('eyes/closed');
  assert.equal(blinking(shut, PSD, `blink={{ seed: 'other', interval: 2 }}`), true);
  assert.equal(blinking(shut, PSD.replace(`half: 'eyes/half' }`, `half: 'eyes/half', seed: 'other', interval: 2 }`)), true);
  // An expression's eyes inherit the portrait's timing.
  const angry = PSD.replace(`half: 'eyes/half' }`, `half: 'eyes/half', seed: 'other', interval: 2 }`);
  const [frame] = render({ portrait: angry, view: 'expression="angry"', frames: [shut] });
  assert.deepEqual(frame.enabledLayers, ['face/angry/eyes/shut']);
});

test('blinks follow the composition frame, not the sequence one', () => {
  const shut = firstFrame('closed', { seed: 'Hana' }, 200);
  const [inside] = render({
    portrait: PSD,
    wrap: ['<Sequence from={137}>', '</Sequence>'],
    frames: [shut],
  });
  assert.deepEqual(inside.enabledLayers, ['eyes/closed']);
});

const IMAGES = `defaultExpression: 'calm',
  expressions: { calm: './calm.png', smile: './smile.png', laugh: './laugh.png' },
  lipSync: { a: './mouth-a.png', i: './mouth-i.png', u: './mouth-u.png', e: './mouth-e.png', o: './mouth-o.png' },
  blink: { closed: { calm: './calm-shut.png', smile: './smile-shut.png' }, half: { calm: './calm-half.png' } }`;

test('an image portrait swaps in its eyes-shut image while it blinks', () => {
  const shut = firstFrame('closed', { seed: 'Hana' });
  const frames = [0, shut - 1, shut];
  assert.deepEqual(render({ portrait: IMAGES, frames }), [['./calm.png'], ['./calm-half.png'], ['./calm-shut.png']]);
  // Without a half-shut image the eyes stay open around the blink; mouths still draw on top.
  assert.deepEqual(render({ portrait: IMAGES, view: 'expression="smile" mouth="a"', frames }), [
    ['./smile.png', './mouth-a.png'],
    ['./smile.png', './mouth-a.png'],
    ['./smile-shut.png', './mouth-a.png'],
  ]);
  // An expression without an eyes-shut image does not blink.
  assert.deepEqual(render({ portrait: IMAGES, view: 'expression="laugh"', frames: [shut] }), [['./laugh.png']]);
  assert.deepEqual(render({ portrait: IMAGES, view: 'blink={false}', frames: [shut] }), [['./calm.png']]);
});

test('an image portrait can draw its eyes-shut images as overlays', () => {
  const shut = firstFrame('closed', { seed: 'Hana' });
  const portrait = IMAGES.replace(`half: { calm: './calm-half.png' } }`, `half: { calm: './calm-half.png' }, overlay: true }`);
  assert.deepEqual(render({ portrait, view: 'mouth="a"', frames: [0, shut] }), [
    ['./calm.png', './mouth-a.png'],
    ['./calm.png', './calm-shut.png', './mouth-a.png'],
  ]);
});
