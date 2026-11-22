import assert from 'node:assert/strict';
import { test } from 'vitest';

import { Easings, keyframes } from '../src/index.js';
import type { ValueKeyframe } from '../src/index.js';

const fps = 30;
const close = (actual: number, expected: number, message?: string) =>
  assert.ok(Math.abs(actual - expected) <= 1e-9, message ?? `${actual} is not ${expected}`);

// A push-in, a drifting hold from frame 30 to 90 (two seconds), then a move.
const cam: ValueKeyframe<{ x: number; y: number; zoom: number }>[] = [
  { frame: 0, value: { x: 960, y: 540, zoom: 1 } },
  { frame: 30, value: { x: 880, y: 520, zoom: 1.2 }, drift: { x: -4, zoom: { log: 0.05 } } },
  { frame: 90, value: { x: 880, y: 520, zoom: 1.2 } },
  { frame: 120, value: { x: 300, y: 400, zoom: 1 } },
];

test('a held value creeps at its rate per second, and the other fields stay still', () => {
  assert.deepEqual(keyframes(30, cam, { fps }), { x: 880, y: 520, zoom: 1.2 });
  const half = keyframes(45, cam, { fps });
  close(half.x, 880 - 4 * 0.5);
  assert.equal(half.y, 520);
  close(half.zoom, 1.2 * Math.exp(0.05 * 0.5));
  // Just before the hold's end key, the drift has nearly run its two seconds.
  close(keyframes(89.5, cam, { fps }).x, 880 - 4 * (59.5 / 30));
});

test('the move after a drifting hold starts where the drift reached', () => {
  const reached = { x: 880 - 4 * 2, zoom: 1.2 * Math.exp(0.05 * 2) };
  const atEnd = keyframes(90, cam, { fps });
  close(atEnd.x, reached.x);
  close(atEnd.zoom, reached.zoom);
  // No jump across the end key.
  assert.ok(Math.abs(keyframes(89.999, cam, { fps }).x - atEnd.x) < 1e-3);
  // Halfway through the linear move, from the drifted value to the next key.
  const mid = keyframes(105, cam, { fps });
  close(mid.x, reached.x + (300 - reached.x) * 0.5);
  close(mid.zoom, reached.zoom + (1 - reached.zoom) * 0.5);
  close(mid.y, 460);
  // The move ends exactly on its key.
  assert.deepEqual(keyframes(120, cam, { fps }), { x: 300, y: 400, zoom: 1 });
});

test('the move after a drift takes its easing', () => {
  const keys = [
    { frame: 0, value: 10, drift: 3 },
    { frame: 30, value: 10 },
    { frame: 60, value: 100, easing: Easings.whip },
  ];
  close(keyframes(45, keys, { fps }), 13 + (100 - 13) * Easings.whip(0.5));
  close(keyframes(50, keys, { fps }), 13 + (100 - 13) * Easings.whip(20 / 30));
});

test('a later hold keeps the drifted value, and drifts on from it with its own rate', () => {
  const keys = [
    { frame: 0, value: 0, drift: 2 },
    { frame: 30, value: 0 },
    { frame: 60, value: 0 },
    { frame: 90, value: 0, drift: -1 },
    { frame: 120, value: 0 },
  ];
  close(keyframes(45, keys, { fps }), 2);
  close(keyframes(60, keys, { fps }), 2);
  close(keyframes(105, keys, { fps }), 2 - 0.5);
  close(keyframes(500, keys, { fps }), 1);
});

test('the last key drifts on forever, as a pure function of the frame', () => {
  const keys = [{ frame: 0, value: 0 }, { frame: 30, value: 5, drift: 1.5 }];
  close(keyframes(30, keys, { fps }), 5);
  close(keyframes(330, keys, { fps }), 5 + 1.5 * 10);
  // Out of order requests give the same answers.
  const frames = [300, 40, 31.5, 300, 40];
  const values = frames.map((f) => keyframes(f, keys, { fps }));
  assert.equal(values[0], values[3]);
  assert.equal(values[1], values[4]);
  close(values[2], 5 + 1.5 * 0.05);
  // Before the first key nothing drifts.
  assert.equal(keyframes(-30, [{ frame: 0, value: 4, drift: 2 }], { fps }), 4);
});

test('a cut after a drifting hold starts from its own key', () => {
  const keys = [
    { frame: 0, value: { x: 100 }, drift: { x: 10 } },
    { frame: 30, value: { x: 100 } },
    { frame: 30, value: { x: 500 } },
    { frame: 60, value: { x: 600 } },
  ];
  close(keyframes(29, keys, { fps }).x, 100 + 10 * (29 / 30));
  assert.deepEqual(keyframes(30, keys, { fps }), { x: 500 });
  assert.deepEqual(keyframes(45, keys, { fps }), { x: 550 });
  // The second key of a cut may drift too.
  const cut = [
    { frame: 0, value: 1 },
    { frame: 10, value: 1 },
    { frame: 10, value: 2, drift: { log: Math.log(2) } },
  ];
  close(keyframes(40, cut, { fps }), 4);
});

test('a field drifts while it holds even when other fields move', () => {
  const keys = [
    { frame: 0, value: { x: 0, zoom: 2 }, drift: { zoom: { log: -0.1 } } },
    { frame: 30, value: { x: 300, zoom: 2 } },
  ];
  const at = keyframes(15, keys, { fps });
  close(at.x, 150);
  close(at.zoom, 2 * Math.exp(-0.05));
});

test('a value with a field named log takes a per-field drift for it', () => {
  const keys = [{ frame: 0, value: { log: 1 }, drift: { log: 3 } }];
  close(keyframes(15, keys, { fps }).log, 2.5);
});

test('a field named like an Object.prototype member holds without a rate of its own', () => {
  const keys = [{ frame: 0, value: { constructor: 5, toString: 2 } }, { frame: 30, value: { constructor: 5, toString: 2 } }];
  assert.deepEqual(keyframes(15, keys, { fps }), { constructor: 5, toString: 2 });
  assert.deepEqual(keyframes(15, keys), { constructor: 5, toString: 2 });
  const partial = [{ frame: 0, value: { constructor: 5, x: 0 }, drift: { x: 2 } }, { frame: 30, value: { constructor: 5, x: 0 } }];
  const at = keyframes(15, partial, { fps });
  assert.equal(at.constructor, 5);
  close(at.x, 1);
});

test('a field whose rate is undefined does not drift, as if it were left out', () => {
  const rate: number | undefined = undefined;
  const keys = [
    { frame: 0, value: { x: 0, y: 0 }, drift: { x: 2, y: rate } },
    { frame: 30, value: { x: 0, y: 10 } },
  ];
  const at = keyframes(15, keys, { fps });
  close(at.x, 1);
  close(at.y, 5);
});

test('keys without drift give the values they gave before, with or without fps', () => {
  const keys = [
    { frame: 0, value: { x: 0, zoom: 1 } },
    { frame: 100, value: { x: 200, zoom: 2 }, easing: Easings.easeInQuad },
    { frame: 120, value: { x: 200, zoom: 2 } },
    { frame: 120, value: { x: 900, zoom: 1 } },
  ];
  for (const frame of [-5, 0, 50, 100, 110, 119.75, 120, 500]) {
    assert.deepEqual(keyframes(frame, keys, { fps }), keyframes(frame, keys));
  }
});

test('rejects a drift that cannot be held or read, with the key index', () => {
  const hold = (drift: unknown) => [{ frame: 0, value: { x: 0, y: 0 }, drift }, { frame: 10, value: { x: 0, y: 0 } }] as never;
  assert.throws(() => keyframes(0, hold({ x: 1 })), /positive fps option to drift \(key 0\)/);
  assert.throws(() => keyframes(0, hold({ x: 1 }), { fps: 0 }), /positive fps option to drift \(key 0\)/);
  assert.throws(() => keyframes(0, hold({ x: NaN }), { fps }), /rate per second or \{ log \}.*\(key 0\)/);
  assert.throws(() => keyframes(0, hold({ x: { log: 1, scale: 2 } }), { fps }), /rate per second or \{ log \}/);
  assert.throws(() => keyframes(0, hold(3), { fps }), /rate per second or \{ log \}/);
  assert.throws(() => keyframes(0, hold({ log: 1 }), { fps }), /rate per second or \{ log \}/);
  assert.throws(() => keyframes(0, hold({ z: 1 }), { fps }), /drift for "z", which the value does not have \(key 0\)/);
  assert.throws(() => keyframes(0, [{ frame: 0, value: 0, drift: { x: 1 } }] as never, { fps }), /rate per second or \{ log \}/);
  assert.throws(
    () => keyframes(0, [{ frame: 0, value: { x: 0 } }, { frame: 10, value: { x: 5 }, drift: { x: 1 } }, { frame: 20, value: { x: 6 } }], { fps }),
    /holds after its key, and "x" moves \(key 1\)/,
  );
  assert.throws(
    () => keyframes(0, [{ frame: 0, value: 0, drift: 1 }, { frame: 10, value: 5 }], { fps }),
    /holds after its key, and the value moves \(key 0\)/,
  );
  assert.throws(
    () => keyframes(0, [{ frame: 0, value: 0 }, { frame: 10, value: 0, drift: 1 }, { frame: 10, value: 3 }], { fps }),
    /holds after its key, and a cut follows \(key 1\)/,
  );
});
