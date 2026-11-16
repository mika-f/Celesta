import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'vitest';

import { Easings, keyframes } from '../src/index.js';
import type { Easing, ValueKeyframe } from '../src/index.js';

const cam: ValueKeyframe<{ x: number; zoom: number }>[] = [
  { frame: 0, value: { x: 0, zoom: 1 } },
  { frame: 100, value: { x: 200, zoom: 2 }, easing: Easings.easeInQuad },
  { frame: 120, value: { x: 200, zoom: 2 } },
  { frame: 120, value: { x: 900, zoom: 1 } },
  { frame: 150, value: { x: 1000, zoom: 1 }, easing: 'ease-out-cubic' },
];

test('interpolates each field with the easing of the key a segment ends on', () => {
  assert.deepEqual(keyframes(50, cam), { x: 200 * 0.25, zoom: 1.25 });
  assert.equal(keyframes(135, cam).x, 900 + 100 * Easings.easeOutCubic(0.5));
  assert.equal(keyframes(5, [{ frame: 0, value: 0 }, { frame: 10, value: 4 }]), 2);
});

test('holds before the first key, after the last, and between equal keys', () => {
  assert.deepEqual(keyframes(-10, cam), { x: 0, zoom: 1 });
  assert.deepEqual(keyframes(999, cam), { x: 1000, zoom: 1 });
  assert.deepEqual(keyframes(110, cam), { x: 200, zoom: 2 });
  assert.equal(keyframes(7, [{ frame: 3, value: 5 }]), 5);
});

test('two keys on one frame cut: the second value is on screen on that frame', () => {
  assert.deepEqual(keyframes(119, cam), { x: 200, zoom: 2 });
  assert.deepEqual(keyframes(119.75, cam), { x: 200, zoom: 2 });
  assert.deepEqual(keyframes(120, cam), { x: 900, zoom: 1 });
  // A cut on the first frame: the second key wins from that frame on, the first before it.
  const lead = [{ frame: 0, value: 1 }, { frame: 0, value: 2 }, { frame: 10, value: 3 }];
  assert.equal(keyframes(-1, lead), 1);
  assert.equal(keyframes(0, lead), 2);
  // A cut on the last frame.
  const tail = [{ frame: 0, value: 1 }, { frame: 10, value: 2 }, { frame: 10, value: 7 }];
  assert.equal(keyframes(9.5, tail), 1.95);
  assert.equal(keyframes(10, tail), 7);
  assert.equal(keyframes(11, tail), 7);
});

test('an overshooting easing overshoots, and returns a new object each call', () => {
  const keys = [{ frame: 0, value: { y: 0 } }, { frame: 10, value: { y: 100 }, easing: Easings.easeOutBack }];
  assert.ok(keyframes(7, keys).y > 100);
  assert.notEqual(keyframes(10, keys), keys[1].value);
  assert.deepEqual(keyframes(10, keys), { y: 100 });
});

test('rejects malformed keys with the key index', () => {
  assert.throws(() => keyframes(0, []), /at least one key/);
  assert.throws(() => keyframes(NaN, [{ frame: 0, value: 0 }]), /finite frame/);
  assert.throws(() => keyframes(0, [{ frame: NaN, value: 0 }]), /finite frame \(key 0\)/);
  assert.throws(() => keyframes(0, [{ frame: 0, value: Infinity }]), /finite value \(key 0\)/);
  assert.throws(() => keyframes(0, [{ frame: 0, value: { x: NaN } }]), /finite value \(key 0\)/);
  assert.throws(() => keyframes(0, [{ frame: 5, value: 0 }, { frame: 4, value: 1 }]), /in order of frame \(key 1\)/);
  assert.throws(() => keyframes(0, [0, 0, 0].map(() => ({ frame: 1, value: 0 }))), /at most two keys on one frame \(key 2\)/);
  assert.throws(() => keyframes(0, [{ frame: 0, value: 0 }, { frame: 1, value: { x: 1 } }] as never), /same shape \(key 1\)/);
  assert.throws(() => keyframes(0, [{ frame: 0, value: { x: 0 } }, { frame: 1, value: { y: 1 } }] as never), /same shape \(key 1\)/);
  assert.throws(() => keyframes(0, [{ frame: 0, value: { x: 0 } }, { frame: 1, value: { x: 1, y: 1 } }] as never), /same shape \(key 1\)/);
  assert.throws(
    () => keyframes(0, [{ frame: 0, value: 0 }, { frame: 1, value: 1, easing: 'ease-sideways' as never }]),
    /unknown easing "ease-sideways" \(key 1\)/,
  );
});

test('every Easing name resolves to the Easings curve of the same name', () => {
  // The names the Rust evaluator knows, from the fixture that pins it.
  const fixture: { cases: { name: string }[] } = JSON.parse(
    readFileSync(new URL('../../../crates/composition/tests/fixtures/keyframe-evaluation.json', import.meta.url), 'utf8'),
  );
  const camelCase = (name: string) => name.replace(/-([a-z])/g, (_, letter: string) => letter.toUpperCase());
  const curves = Easings as unknown as Record<string, (t: number) => number>;
  const names = fixture.cases.map((c) => c.name).filter((name) => camelCase(name) in curves);
  assert.ok(names.length >= 34);
  for (const name of names) {
    const keys = [{ frame: 0, value: 0 }, { frame: 10, value: 1, easing: name as Easing }];
    assert.equal(keyframes(3, keys), curves[camelCase(name)](0.3));
  }
});

test('a value that is neither a number nor an object of numbers throws', () => {
  assert.throws(() => keyframes(0, [{ frame: 0, value: [960, 540] as never }]), /a number or an object of numbers \(key 0\)/);
  assert.throws(() => keyframes(0, [{ frame: 0, value: null as never }]), /a number or an object of numbers \(key 0\)/);
  assert.throws(() => keyframes(0, [{ frame: 0, value: 0 }, { frame: 1, value: '1' as never }]), /a number or an object of numbers \(key 1\)/);
  assert.throws(() => keyframes(0, [{ frame: 0, value: { x: '1' } as never }]), /finite value \(key 0\)/);
});

test('a number result is typed as a number, not as the keys\' literals', () => {
  const value = keyframes(5, [{ frame: 0, value: 0 }, { frame: 10, value: 4 }]);
  // Compiles only when the result is `number`: `0 | 4` has no overlap with 2.
  assert.equal(value === 2, true);
});

test('a factory name is not an easing name', () => {
  assert.throws(
    () => keyframes(0, [{ frame: 0, value: 0 }, { frame: 1, value: 1, easing: 'cubic-bezier' as never }]),
    /unknown easing "cubic-bezier" \(key 1\)/,
  );
});
