import { expect, test } from 'vitest';

import { interpolate, spring } from '../src/index.js';

test('interpolate maps, extends, and clamps finite inputs', () => {
  expect(interpolate(5, [0, 10], [0, 100])).toBe(50);
  expect(interpolate(15, [0, 10], [0, 100])).toBe(150);
  expect(interpolate(-5, [0, 10], [0, 100], { extrapolateLeft: 'clamp' })).toBe(0);
  expect(interpolate(-5, [0, 10], [0, 100], { extrapolateLeft: 'identity' })).toBe(-5);
});

test('interpolate rejects a non-finite input instead of returning NaN', () => {
  for (const input of [Number.NaN, Infinity, undefined as unknown as number]) {
    expect(() => interpolate(input, [0, 1], [0, 1], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }), String(input))
      .toThrow(/interpolate\(\) requires a finite input/);
  }
});

test('interpolate rejects non-finite or missing range entries', () => {
  // `undefined` and the hole stand in for a misspelled field.
  expect(() => interpolate(0.5, [0, Number.NaN], [0, 1])).toThrow(/finite inputRange values, not NaN at index 1/);
  expect(() => interpolate(0.5, [0, undefined as unknown as number], [0, 1])).toThrow(/finite inputRange values/);
  // eslint-disable-next-line no-sparse-arrays
  expect(() => interpolate(0.5, [0, , 2] as number[], [0, 1, 2])).toThrow(/finite inputRange values, not undefined at index 1/);
  expect(() => interpolate(0.5, [0, 1], [0, Number.NaN])).toThrow(/finite outputRange values, not NaN at index 1/);
  expect(() => interpolate(0.5, [0, 1], [Infinity, 1])).toThrow(/finite outputRange values/);
  expect(() => interpolate(0.5, [0, 0], [0, 1])).toThrow(/strictly increasing/);
});

test('spring settles toward its target and holds from before its delay', () => {
  expect(spring({ frame: 0, fps: 30 })).toBe(0);
  expect(spring({ frame: 5, fps: 30, delay: 10, from: 2 })).toBe(2);
  expect(spring({ frame: 300, fps: 30 })).toBeCloseTo(1, 5);
});

test('spring rejects a non-finite frame or fps', () => {
  expect(() => spring({ frame: 10, fps: Number.NaN })).toThrow(/spring\(\) requires a positive fps/);
  expect(() => spring({ frame: 10, fps: Infinity })).toThrow(/spring\(\) requires a positive fps/);
  expect(() => spring({ frame: 10, fps: 0 })).toThrow(/spring\(\) requires a positive fps/);
  expect(() => spring({ frame: Number.NaN, fps: 30 })).toThrow(/spring\(\) requires a finite frame, not NaN/);
});

test('spring rejects a NaN delay, from, to, or durationInFrames', () => {
  expect(() => spring({ frame: 10, fps: 30, delay: Number.NaN })).toThrow(/spring\(\) requires a finite delay, not NaN/);
  expect(() => spring({ frame: 10, fps: 30, from: Number.NaN })).toThrow(/spring\(\) requires a finite from, not NaN/);
  expect(() => spring({ frame: 10, fps: 30, to: Infinity })).toThrow(/spring\(\) requires a finite to, not Infinity/);
  expect(() => spring({ frame: 10, fps: 30, durationInFrames: Number.NaN }))
    .toThrow(/spring\(\) requires durationInFrames to be a number, not NaN/);
  // An endless spring is the same as none.
  expect(spring({ frame: 10, fps: 30, durationInFrames: Infinity })).toBe(spring({ frame: 10, fps: 30 }));
});
