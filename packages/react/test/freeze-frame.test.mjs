// <FreezeFrame frame>: children redrawn as the composition looked at an
// absolute frame, with portraits scoped to the copy and no sound. Run after
// `pnpm run build`.

import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';

import {
  Assets,
  Audio,
  Character,
  CharacterView,
  Composition,
  Dialogue,
  FreezeFrame,
  Group,
  Sequence,
  Text,
  Video,
  blinkPhase,
  useCurrentFrame,
  useVideoConfig,
} from '../dist/index.js';
import { mount } from '../dist/render.js';

const h = React.createElement;
const FPS = 30;
const at = (composition, frame) => composition.renderAt({ value: frame, timescale: FPS }, null);
const root = (...children) => h(Composition, { width: 10, height: 10, fps: FPS, durationInFrames: 120 }, ...children);

function texts(layers) {
  return layers.flatMap((layer) =>
    layer.content.type === 'text' ? [layer.content.text] : layer.content.type === 'group' ? texts(layer.content.layers) : [],
  );
}

function images(layers) {
  return layers.flatMap((layer) =>
    layer.content.type === 'image' ? [layer.content.asset.id] : layer.content.type === 'group' ? images(layer.content.layers) : [],
  );
}

/** The layer with `id`, depth first. */
function find(layers, id) {
  for (const layer of layers) {
    if (layer.id === id) return layer;
    if (layer.content.type === 'group') {
      const found = find(layer.content.layers, id);
      if (found) return found;
    }
  }
  return undefined;
}

function portrait(name, extra = {}) {
  return {
    defaultExpression: 'calm',
    expressions: { calm: `${name}-calm.png`, smile: `${name}-smile.png`, sad: `${name}-sad.png` },
    ...extra,
  };
}

test('children see the composition clock at the frozen frame, even inside a Sequence', () => {
  function Clock({ name }) {
    const frame = useCurrentFrame();
    const { durationInFrames } = useVideoConfig();
    return h(Text, null, `${name}:${frame}/${durationInFrames}`);
  }
  const composition = mount(() => root(
    h(FreezeFrame, { frame: 40 }, h(Clock, { name: 'root' })),
    h(Sequence, { from: 60, durationInFrames: 10 },
      h(FreezeFrame, { frame: 100 }, h(Clock, { name: 'nested' })),
    ),
  ));
  assert.deepEqual(texts(at(composition, 65).scene.layers), ['root:40/120', 'nested:100/120']);
  assert.deepEqual(texts(at(composition, 5).scene.layers), ['root:40/120']);
});

test('inner sequences open against the frozen frame, even inside a short Sequence', () => {
  const World = () => h(React.Fragment, null,
    h(Sequence, { durationInFrames: 50 }, h(Text, null, 'intro')),
    h(Sequence, { from: 50 }, h(Text, null, 'main')),
  );
  function Root() {
    const frame = useCurrentFrame();
    return root(
      h(Sequence, { from: 10, durationInFrames: 5 },
        h(FreezeFrame, { frame: frame < 12 ? 20 : 80 }, h(World)),
      ),
    );
  }
  const composition = mount(Root);
  assert.deepEqual(texts(at(composition, 0).scene.layers), []);
  assert.deepEqual(texts(at(composition, 11).scene.layers), ['intro']);
  assert.deepEqual(texts(at(composition, 13).scene.layers), ['main']);
});

test('frozen children contribute no audio', () => {
  const composition = mount(() => root(
    h(Audio, { src: 'live.wav' }),
    h(FreezeFrame, { frame: 5 },
      h(Audio, { src: 'frozen.wav' }),
      h(Sequence, { from: 0 }, h(Audio, { src: 'frozen-sequence.wav' })),
    ),
  ));
  assert.deepEqual(at(composition, 3).audio.map((clip) => clip.src), ['live.wav']);
  assert.deepEqual([...new Set(composition.collectAudio().map((clip) => clip.src))], ['live.wav']);
});

test('a frozen copy of a view and its line leaves the live view and its ref alone', () => {
  const mira = React.createRef();
  const view = React.createRef();
  const Cast = ({ expression }) => h(React.Fragment, null,
    h(CharacterView, { ref: view, character: mira }),
    h(Dialogue, { character: view, expression, audio: `${expression}.wav` }, expression),
  );
  function Root() {
    const frame = useCurrentFrame();
    return root(
      h(Assets, null, h(Character, { ref: mira, name: 'mira', portrait: portrait('mira') })),
      h(Group, { id: 'live' }, h(Cast, { expression: 'smile' })),
      frame < 2 ? h(FreezeFrame, { id: 'copy', frame: 0 }, h(Cast, { expression: 'sad' })) : null,
    );
  }
  const composition = mount(Root);
  for (const frame of [0, 1]) {
    const { scene, audio } = at(composition, frame);
    assert.deepEqual(images([find(scene.layers, 'live')]), ['mira-smile.png'], `live view at frame ${frame}`);
    assert.deepEqual(images([find(scene.layers, 'copy')]), ['mira-sad.png'], `frozen view at frame ${frame}`);
    assert.deepEqual(texts(scene.layers), ['smile', 'sad']);
    assert.deepEqual(audio.map((clip) => clip.src), ['smile.wav']);
  }
  const { scene } = at(composition, 2);
  assert.equal(view.current?.type, 'character-view', 'unmounting the copy keeps the live ref attached');
  assert.deepEqual(images(scene.layers), ['mira-smile.png']);
});

test('frozen lip sync and blinking show what the original frame showed', () => {
  const timing = { interval: 1, duration: 0.2 };
  let blinkFrame = 30;
  while (blinkPhase(blinkFrame, FPS, { seed: 'mira', ...timing }) !== 'closed') blinkFrame += 1;
  let openFrame = 0;
  while (blinkPhase(openFrame, FPS, { seed: 'mira', ...timing }) !== 'open') openFrame += 1;
  const track = {
    durationInSeconds: 10,
    mouthAtSeconds: (seconds) => ['a', 'i', 'u', 'e', 'o'][Math.round(seconds * FPS) % 5],
    mouthAtFrame: (frame) => ['a', 'i', 'u', 'e', 'o'][frame % 5],
  };
  const lipSync = { a: 'a.png', i: 'i.png', u: 'u.png', e: 'e.png', o: 'o.png', closed: 'closed.png' };
  const blink = { ...timing, closed: { calm: 'calm-closed.png' } };
  const compose = (wrap) => {
    const mira = React.createRef();
    const view = React.createRef();
    const World = () => h(Sequence, { from: 30 },
      h(CharacterView, { ref: view, character: mira }),
      h(Dialogue, { character: view, lipSync: track }, 'line'),
    );
    return mount(() => root(
      h(Assets, null, h(Character, { ref: mira, name: 'mira', portrait: portrait('mira', { lipSync, blink }) })),
      wrap(h(World)),
    ));
  };
  const live = compose((world) => world);
  for (const frame of [blinkFrame, blinkFrame + 1, blinkFrame + 2]) {
    const frozen = compose((world) => h(FreezeFrame, { frame }, world));
    const original = images(at(live, frame).scene.layers);
    assert.deepEqual(images(at(frozen, openFrame).scene.layers), original, `frozen frame ${frame}`);
  }
  assert.ok(images(at(live, blinkFrame).scene.layers).includes('calm-closed.png'));
});

test('a frozen line must refer to a view inside the same, innermost freeze', () => {
  const setup = (body) => {
    const mira = React.createRef();
    const view = React.createRef();
    return mount(() => root(
      h(Assets, null, h(Character, { ref: mira, name: 'mira', portrait: portrait('mira') })),
      body({ view: h(CharacterView, { ref: view, character: mira }), line: h(Dialogue, { character: view }, 'hi') }),
    ));
  };
  const message = /<Dialogue> inside <FreezeFrame> must refer to a <CharacterView> rendered inside the same <FreezeFrame>/;
  assert.throws(() => at(setup(({ view, line }) => h(React.Fragment, null, view, h(FreezeFrame, { frame: 0 }, line))), 0), message);
  assert.throws(
    () => at(setup(({ view, line }) => h(FreezeFrame, { frame: 0 }, view, h(FreezeFrame, { frame: 0 }, line))), 0),
    message,
  );
  const nested = setup(({ view, line }) => h(FreezeFrame, { frame: 0 }, h(FreezeFrame, { frame: 0 }, view, line)));
  assert.deepEqual(texts(at(nested, 0).scene.layers), ['hi']);
});

test('a frozen video follows the frozen frame and authored ids are prefixed', () => {
  const composition = mount(() => root(
    h(Video, { id: 'clip', src: 'clip.mp4', startFrom: 1 }),
    h(FreezeFrame, { id: 'thumb', frame: 45 }, h(Video, { id: 'clip', src: 'clip.mp4', startFrom: 1 })),
  ));
  const { scene } = at(composition, 3);
  assert.equal(find(scene.layers, 'clip').content.timing.sourceTimeSeconds, 1.1);
  assert.equal(find(scene.layers, 'thumb/clip').content.timing.sourceTimeSeconds, 2.5);
  assert.throws(() => mount(() => root(h(FreezeFrame, { frame: Number.NaN }))), /<FreezeFrame> requires a finite `frame`/);
});
