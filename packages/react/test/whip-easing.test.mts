import assert from 'node:assert/strict';
import { test } from 'vitest';

import { Easings, keyframes } from '../src/index.js';

const { whip } = Easings;

test('whip is exactly 0 and 1 at its ends and passes through the middle', () => {
  assert.equal(whip(0), 0);
  assert.equal(whip(1), 1);
  assert.equal(whip(0.5), 0.5);
});

test('whip is symmetric and never decreases', () => {
  let previous = 0;
  for (let step = 1; step <= 200; step += 1) {
    const t = step / 200;
    assert.ok(whip(t) >= previous, `whip falls at ${t}`);
    assert.ok(Math.abs(whip(t) + whip(1 - t) - 1) <= 1e-12, `whip is not symmetric at ${t}`);
    previous = whip(t);
  }
});

test('whip moves most of the way in its middle third, faster than ease-in-out-expo', () => {
  assert.ok(whip(2 / 3) - whip(1 / 3) > 0.98);
  assert.ok(whip(0.25) < 0.001);
  const speed = (curve: (t: number) => number) => (curve(0.5 + 1e-6) - curve(0.5 - 1e-6)) / 2e-6;
  assert.ok(Math.abs(speed(whip) - 7 / Math.tanh(7)) < 1e-4);
  assert.ok(speed(whip) > speed(Easings.easeInOutExpo));
  assert.ok(whip(1 / 3) < Easings.easeInOutExpo(1 / 3));
});

test('keyframes() takes whip by its JSON name', () => {
  const keys = [{ frame: 0, value: 0 }, { frame: 10, value: 100, easing: 'whip' as const }];
  assert.equal(keyframes(4, keys), 100 * whip(0.4));
});
