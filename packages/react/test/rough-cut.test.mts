import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';

import { Composition, ScenePlaceholder, Scenes, Sequence, Text, defineScenes, listChapters } from '../src/index.js';
import type { Layer } from '../src/index.js';
import { mount } from '../src/render.js';
import type { MountedComposition } from '../src/render.js';

const h = React.createElement;
const at = (composition: MountedComposition, frame: number) => composition.renderAt({ value: frame, timescale: 30 }, null);
const texts = (layers: Layer[]): string[] => layers.flatMap((layer) =>
  layer.content.type === 'text' ? [layer.content.text] : texts(layer.content.type === 'group' ? layer.content.layers : []),
);
const rects = (layers: Layer[]): Layer[] => layers.flatMap((layer) =>
  layer.content.type === 'rect' ? [layer] : rects(layer.content.type === 'group' ? layer.content.layers : []),
);

const ROUGH = defineScenes([
  { id: 'open', name: 'Cold open', durationInFrames: 60, desc: 'A card slides onto the blueprint.', status: 'rough' },
  { id: 'built', name: 'Built', durationInFrames: 30, Scene: () => h(Text, null, 'built scene') },
  { id: 'wip', name: 'Work in progress', durationInFrames: 30, status: 'rough', Scene: () => h(Text, null, 'hidden') },
  { id: 'bare', durationInFrames: 30 },
]);

const mountRough = () => mount(() => h(Composition, { width: 1920, height: 1080, fps: 30, durationInFrames: ROUGH.durationInFrames, chapters: ROUGH },
  h(Scenes, { scenes: ROUGH }),
));

test('defineScenes keeps desc and status and checks them', () => {
  assert.equal(ROUGH.scenes[0].desc, 'A card slides onto the blueprint.');
  assert.equal(ROUGH.scenes[0].status, 'rough');
  assert.equal(ROUGH.scenes[1].status, undefined);
  assert.throws(() => defineScenes([{ id: 'a', durationInFrames: 1, desc: 3 as unknown as string }]), /string desc/);
  assert.throws(() => defineScenes([{ id: 'a', durationInFrames: 1, status: 'draft' as 'rough' }]), /'rough' or 'final'/);
});

test('Scenes draws a placeholder card for rough scenes and scenes without a Scene', () => {
  const composition = mountRough();
  try {
    assert.deepEqual(texts(at(composition, 0).scene.layers), [
      'Scene 1 of 4',
      'Rough cut',
      'Cold open',
      'A card slides onto the blueprint.',
      '2 s · 60 frames',
      '0 s',
    ]);
    assert.deepEqual(texts(at(composition, 60).scene.layers), ['built scene']);
    // A rough scene's own Scene stays hidden until it is final.
    const wip = texts(at(composition, 95).scene.layers);
    assert.ok(wip.includes('Work in progress') && wip.includes('Scene 3 of 4'), wip.join(' | '));
    assert.ok(!wip.includes('hidden'));
    const bare = texts(at(composition, 125).scene.layers);
    assert.deepEqual(bare.slice(0, 3), ['Scene 4 of 4', 'No Scene yet', 'bare']);
  } finally {
    composition.dispose();
  }
});

test('the placeholder is a pure function of the frame', () => {
  const composition = mountRough();
  try {
    const bar = (frame: number) => {
      const drawn = rects(at(composition, frame).scene.layers);
      // The ground, the track, then the filled bar once it has started (the
      // dashed frame is drawn as a path).
      return drawn.length === 3 ? (drawn[2].content as { width: number }).width : 0;
    };
    const track = (rects(at(composition, 0).scene.layers)[1].content as { width: number }).width;
    assert.equal(bar(0), 0);
    assert.equal(bar(59), track);
    const middle = bar(30);
    assert.ok(middle > 0 && middle < track);
    // Out of order and again: the same picture.
    assert.equal(bar(59), track);
    assert.equal(bar(30), middle);
    assert.deepEqual(at(composition, 30).scene, at(composition, 30).scene);
  } finally {
    composition.dispose();
  }
});

test('chapters carry desc and status to the editor', () => {
  assert.deepEqual(listChapters(ROUGH).slice(0, 2), [
    { id: 'open', name: 'Cold open', from: 0, durationInFrames: 60, desc: 'A card slides onto the blueprint.', status: 'rough' },
    { id: 'built', name: 'Built', from: 60, durationInFrames: 30 },
  ]);
  const composition = mountRough();
  try {
    assert.deepEqual(
      (composition.config.chapters ?? []).map(({ id, status }) => `${id}:${status ?? 'final'}`),
      ['open:rough', 'built:final', 'wip:rough', 'bare:final'],
    );
  } finally {
    composition.dispose();
  }
  assert.throws(() => listChapters([{ id: 'a', from: 0, durationInFrames: 1, desc: 1 as unknown as string }]), /string desc/);
  assert.throws(() => listChapters([{ id: 'a', from: 0, durationInFrames: 1, status: 'done' as 'final' }]), /'rough' or 'final'/);
});

test('ScenePlaceholder scales to vertical canvases and reads its own Sequence', () => {
  const composition = mount(() => h(Composition, { width: 1080, height: 1920, fps: 30, durationInFrames: 40 },
    h(Sequence, { from: 10, durationInFrames: 30 },
      h(ScenePlaceholder, { name: 'Phone', durationInFrames: 30, label: 'Rough cut' }),
    ),
  ));
  try {
    const drawn = at(composition, 10).scene.layers;
    assert.deepEqual(texts(drawn), ['Rough cut', 'Phone', '1 s · 30 frames', '0 s']);
    const ground = rects(drawn)[0].content as { width: number; height: number };
    assert.deepEqual([ground.width, ground.height], [1080, 1920]);
    assert.deepEqual(texts(at(composition, 39).scene.layers).at(-1), '0.97 s');
  } finally {
    composition.dispose();
  }
});
