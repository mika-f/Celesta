import assert from 'node:assert/strict';
import test from 'node:test';
import React from 'react';

import { Audio, Composition, Font, Group, Sequence, useCurrentFrame, useTextMetrics } from '../dist/index.js';
import { mount } from '../dist/render.js';
import { setTextMeasurer } from '../dist/text-measure.js';

const h = React.createElement;

test('audio collection preserves every report, font-dependent hooks, state and subsequent renders', () => {
  const measured = [];
  setTextMeasurer(async () => { throw new Error('unexpected async measurement'); }, (request) => {
    measured.push(request);
    return { width: request.text.length * (request.fonts.length ? 20 : 10) };
  });
  function Sound() {
    const frame = useCurrentFrame();
    const [src] = React.useState('state.wav');
    const metrics = useTextMetrics('x'.repeat(frame % 4 + 1));
    return metrics.width > 30 ? h(Audio, { src, volume: frame / 10 }) : null;
  }
  const Root = () => {
    const frame = useCurrentFrame();
    return h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 30 },
      h(Group, null,
        h(Audio, { src: 'same.wav' }), h(Audio, { src: 'same.wav' }),
        frame === 17 && h(Audio, { src: 'one-frame.wav' }),
        h(Sequence, { from: 5, durationInFrames: 20 },
          h(Sequence, { from: -3 }, h(Sound)))),
      frame >= 15 && h(Font, { src: './font.ttf' }));
  };
  const reference = mount(Root);
  const batched = mount(Root);
  for (let pass = 0; pass < 2; pass++) {
    measured.length = 0;
    const expected = Array.from({ length: 30 }, (_, frame) =>
      reference.renderAt({ value: frame, timescale: 30 }, null).audio).flat();
    const expectedMeasurements = [...measured];
    measured.length = 0;
    const actual = batched.collectAudio();
    assert.deepEqual(actual, expected);
    assert.deepEqual(measured, expectedMeasurements);
    assert.equal(actual.filter(({ src }) => src === 'same.wav').length, 60);
    assert.equal(actual.filter(({ src }) => src === 'one-frame.wav').length, 1);
  }
  assert.deepEqual(batched.renderAt({ value: 0, timescale: 30 }, null),
    reference.renderAt({ value: 0, timescale: 30 }, null));
});

test('audio collection skips visual layer construction but propagates render failures', () => {
  const Root = () => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 3 },
    // A visual prop that throws if the layer builder accesses it.
    h('rect', { shadow: { color: '#000000', get blur() { throw new Error('visual layer was built'); } } }),
    h(Audio, { src: 'voice.wav' }));
  assert.equal(mount(Root).collectAudio().length, 3);
  assert.throws(() => mount(Root).renderAt({ value: 0, timescale: 30 }, null), /visual layer was built/);
  function Broken() {
    if (useCurrentFrame() === 2) throw new Error('late audio render failure');
    return null;
  }
  assert.throws(() => mount(() => h(Composition,
    { width: 100, height: 100, fps: 30, durationInFrames: 3 }, h(Broken))).collectAudio(),
  /late audio render failure/);
});
