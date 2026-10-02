import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';

import { Audio, Composition, Sequence, Series, Text, useCurrentFrame, useVideoConfig } from '../dist/index.js';
import { mount } from '../dist/render.js';

const h = React.createElement;
const at = (composition, frame) => composition.renderAt({ value: frame, timescale: 30 }, null);
const texts = (layers) => layers.flatMap((layer) =>
  layer.content.type === 'text' ? [layer.content.text] : texts(layer.content.layers ?? []),
);

test('Sequence never evaluates children outside its window, including after seeking', () => {
  const seen = [];
  function Later() {
    const frame = useCurrentFrame();
    const { durationInFrames } = useVideoConfig();
    assert.ok(frame >= 0 && frame < durationInFrames, `inactive local frame ${frame}`);
    seen.push(frame);
    return h(Text, null, '.'.repeat(frame % 3));
  }
  const composition = mount(() => h(Composition, { width: 1920, height: 1080, fps: 30, durationInFrames: 120 },
    h(Sequence, { from: 60, durationInFrames: 60 }, h(Later)),
  ));
  assert.deepEqual(seen, [], 'inactive children also stay unmounted during config discovery');
  for (const frame of [0, 10, 59, 60, 70, 119, 120, 10, 70]) {
    const { scene, audio } = at(composition, frame);
    assert.deepEqual(texts(scene.layers), frame >= 60 && frame < 120 ? ['.'.repeat((frame - 60) % 3)] : []);
    assert.deepEqual(audio, []);
  }
  assert.deepEqual(seen, [0, 10, 59, 10]);
});

test('nested sequences retain default durations, clipped audio, and conditional hooks', () => {
  const seen = [];
  function Sound({ name }) {
    const frame = useCurrentFrame();
    const { durationInFrames } = useVideoConfig();
    const [src] = React.useState(`${name}.wav`);
    assert.ok(frame >= 0 && frame < durationInFrames);
    seen.push(`${name}:${frame}:${durationInFrames}`);
    return frame % 2 === 0 ? h(Audio, { src, startFrom: 1 }) : null;
  }
  const composition = mount(() => h(Composition, { width: 10, height: 10, fps: 30, durationInFrames: 120 },
    h(Sequence, { from: 30, durationInFrames: 60 },
      h(Sequence, { from: -15 }, h(Sound, { name: 'early' })),
      h(Sequence, { from: 15 }, h(Sound, { name: 'late' })),
    ),
  ));
  for (const frame of [0, 29, 30, 31, 44, 45, 46, 89, 90]) {
    const { audio } = at(composition, frame);
    const expected = [];
    if (frame >= 30 && frame < 90 && (frame - 15) % 2 === 0) {
      expected.push({ src: 'early.wav', sourceStart: 1.5, playbackRate: 1, volume: 1, muted: false, start: 1, duration: 2 });
    }
    if (frame >= 45 && frame < 90 && (frame - 45) % 2 === 0) {
      expected.push({ src: 'late.wav', sourceStart: 1, playbackRate: 1, volume: 1, muted: false, start: 1.5, duration: 1.5 });
    }
    assert.deepEqual(audio, expected, `audio at frame ${frame}`);
  }
  assert.deepEqual(seen, [
    'early:15:75', 'early:16:75', 'early:29:75', 'early:30:75', 'late:0:45',
    'early:31:75', 'late:1:45', 'early:74:75', 'late:44:45',
  ]);
});

test('Series evaluates only active scenes and keeps audio at overlapping boundaries', () => {
  const seen = [];
  function Scene({ name }) {
    const frame = useCurrentFrame();
    const { durationInFrames } = useVideoConfig();
    assert.ok(frame >= 0 && frame < durationInFrames);
    seen.push(`${name}:${frame}`);
    return h(React.Fragment, null, h(Text, null, `${name}:${frame}`), h(Audio, { src: `${name}.wav` }));
  }
  const composition = mount(() => h(Composition, { width: 10, height: 10, fps: 30, durationInFrames: 120 },
    h(Series, null,
      h(Series.Sequence, { durationInFrames: 60 }, h(Scene, { name: 'a' })),
      h(Series.Sequence, { durationInFrames: 60, offset: -30 }, h(Scene, { name: 'b' })),
    ),
  ));
  seen.length = 0;
  for (const frame of [0, 29, 30, 59, 60, 89, 90, 0]) {
    const { scene, audio } = at(composition, frame);
    const expected = [];
    if (frame < 60) expected.push(`a:${frame}`);
    if (frame >= 30 && frame < 90) expected.push(`b:${frame - 30}`);
    assert.deepEqual(texts(scene.layers), expected);
    assert.deepEqual(audio, expected.map((label) => ({
      src: `${label[0]}.wav`, sourceStart: 0, playbackRate: 1, volume: 1, muted: false,
      start: label[0] === 'a' ? 0 : 1, duration: 2,
    })));
    assert.deepEqual(seen.splice(0), expected, `evaluated scenes at frame ${frame}`);
  }
});
