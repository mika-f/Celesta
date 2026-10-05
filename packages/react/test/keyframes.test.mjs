import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';

import { Audio, Composition, Sequence, frameKeyframes } from '../dist/index.js';
import { mount } from '../dist/render.js';

const h = React.createElement;
const us = (value) => ({ value, timescale: 1_000_000 });

test('frameKeyframes converts frames to microsecond times and keeps easing', () => {
  assert.deepEqual(
    frameKeyframes([
      { frame: 0, value: 0 },
      { frame: 15, value: 0.8, easing: 'ease-out' },
    ], { fps: 30 }),
    {
      type: 'keyframes',
      keyframes: [
        { time: us(0), value: 0 },
        { time: us(500_000), value: 0.8, easing: 'ease-out' },
      ],
    },
  );
});

test('frameKeyframes rounds to the nearest microsecond at fractional fps', () => {
  const { keyframes } = frameKeyframes([{ frame: 1, value: 1 }, { frame: 2.5, value: 0 }], { fps: 30000 / 1001 });
  // 1001/30000 s = 33366.66… µs; 2.5 frames = 83416.66… µs.
  assert.deepEqual(keyframes.map(({ time }) => time), [us(33_367), us(83_417)]);
});

test('frameKeyframes measures keys from origin, allowing keys before it', () => {
  const { keyframes } = frameKeyframes([
    { frame: 30, value: 0.5 },
    { frame: 45, value: 0 },
    { frame: 60, value: 1 },
  ], { fps: 30, origin: 45 });
  assert.deepEqual(keyframes.map(({ time }) => time), [us(-500_000), us(0), us(500_000)]);
});

test('frameKeyframes keeps keys on the same frame as a cut and a single key as a constant', () => {
  const cut = frameKeyframes([
    { frame: 0, value: 1 },
    { frame: 30, value: 1 },
    { frame: 30, value: 0.2 },
  ], { fps: 30 });
  assert.deepEqual(cut.keyframes.map(({ value }) => value), [1, 1, 0.2]);
  assert.deepEqual(frameKeyframes([{ frame: 10, value: 0.5 }], { fps: 30 }).keyframes, [{ time: us(333_333), value: 0.5 }]);
});

test('frameKeyframes rejects empty, unordered, and non-finite keys and invalid fps', () => {
  assert.throws(() => frameKeyframes([], { fps: 30 }), /at least one key/);
  assert.throws(
    () => frameKeyframes([{ frame: 10, value: 1 }, { frame: 5, value: 0 }], { fps: 30 }),
    /in order of frame \(key 1\)/,
  );
  assert.throws(() => frameKeyframes([{ frame: NaN, value: 1 }], { fps: 30 }), /finite frame \(key 0\)/);
  assert.throws(() => frameKeyframes([{ frame: 0, value: Infinity }], { fps: 30 }), /finite value \(key 0\)/);
  assert.throws(() => frameKeyframes([{ frame: 0, value: 1 }], { fps: 30, origin: NaN }), /finite origin/);
  for (const fps of [0, -30, NaN, Infinity]) {
    assert.throws(() => frameKeyframes([{ frame: 0, value: 1 }], { fps }), /positive fps/);
  }
});

test('keys written in composition frames line up with audio in nested sequences', () => {
  // The voice starts at frame 45; its keys count from the composition's
  // start, so origin is 45 and the fade starts at the clip's local zero.
  const volume = frameKeyframes([{ frame: 45, value: 0 }, { frame: 51, value: 1 }], { fps: 30, origin: 45 });
  const composition = mount(() => h(Composition, { width: 10, height: 10, fps: 30, durationInFrames: 90 },
    h(Sequence, { from: 30 }, h(Sequence, { from: 15 }, h(Audio, { src: 'voice.wav', volume }))),
  ));
  const { audio } = composition.renderAt({ value: 60, timescale: 30 }, null);
  assert.equal(audio.length, 1);
  assert.equal(audio[0].start, 1.5);
  assert.deepEqual(audio[0].volume, volume);
});
