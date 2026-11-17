import assert from 'node:assert/strict';
import { test } from 'vitest';

import { Easings } from '../src/index.js';

const sweep = Array.from({ length: 101 }, (_, i) => i / 100);

test('back() and elastic() defaults are the named curves, bit for bit', () => {
  for (const t of sweep) {
    assert.equal(Easings.back().easeIn(t), Easings.easeInBack(t));
    assert.equal(Easings.back().easeOut(t), Easings.easeOutBack(t));
    assert.equal(Easings.back().easeInOut(t), Easings.easeInOutBack(t));
    assert.equal(Easings.elastic().easeIn(t), Easings.easeInElastic(t));
    assert.equal(Easings.elastic().easeOut(t), Easings.easeOutElastic(t));
    assert.equal(Easings.elastic().easeInOut(t), Easings.easeInOutElastic(t));
  }
});

test('back(overshoot) overshoots by more with a larger value, and 0 does not overshoot', () => {
  const peak = (f: (t: number) => number) => Math.max(...sweep.map(f));
  assert.ok(peak(Easings.back(3).easeOut) > peak(Easings.back(1.4).easeOut));
  assert.ok(peak(Easings.back(1.4).easeOut) > 1);
  assert.ok(peak(Easings.back(0).easeOut) <= 1);
  assert.throws(() => Easings.back(-1), /overshoot/);
  assert.throws(() => Easings.back(NaN), /overshoot/);
});

test('cubicBezier follows CSS cubic-bezier()', () => {
  const ease = Easings.cubicBezier(0.25, 0.1, 0.25, 1); // CSS `ease`
  assert.ok(Math.abs(ease(0.5) - 0.8024033877) < 1e-6);
  const linear = Easings.cubicBezier(0.3, 0.3, 0.7, 0.7);
  for (const t of sweep) assert.ok(Math.abs(linear(t) - t) < 1e-6);
  assert.throws(() => Easings.cubicBezier(-0.1, 0, 1, 1), /x1/);
  assert.throws(() => Easings.cubicBezier(0, 0, 1.1, 1), /x2/);
  assert.throws(() => Easings.cubicBezier(0, NaN, 1, 1), /finite/);
});

test('cubicBezier stays finite and exact at the ends for extreme control points', () => {
  const points: [number, number, number, number][] = [[0, 0, 1, 1], [0, 1.8, 1, -0.8], [1, 0, 0, 1], [0.5, -2, 0.5, 3]];
  for (const args of points) {
    const f = Easings.cubicBezier(...args);
    assert.ok(f(0) === 0 && f(1) === 1);
    for (const t of sweep) assert.ok(Number.isFinite(f(t)));
  }
});

test('a cubicBezier that overshoots leaves 0–1 inside the span', () => {
  const f = Easings.cubicBezier(0.34, 1.56, 0.64, 1);
  assert.ok(Math.max(...sweep.map(f)) > 1);
});

test('steps(count) holds each step and ends on 1', () => {
  const f = Easings.steps(4);
  assert.deepEqual([0, 0.24, 0.25, 0.5, 0.99, 1].map(f), [0, 0, 0.25, 0.5, 0.75, 1]);
  for (const bad of [0, -1, 1.5, NaN]) assert.throws(() => Easings.steps(bad), /positive integer/);
});

test('every factory curve starts at exactly 0 and ends at exactly 1', () => {
  const curves = [
    Easings.steps(3),
    Easings.cubicBezier(0.4, 0, 0.2, 1),
    ...Object.values(Easings.back(2.5)),
    ...Object.values(Easings.elastic({ amplitude: 1.5, period: 0.5 })),
  ];
  for (const f of curves) assert.ok(f(0) === 0 && f(1) === 1);
});

test('elastic takes an amplitude and a period', () => {
  const peak = (f: (t: number) => number) => Math.max(...sweep.map(f));
  assert.ok(peak(Easings.elastic({ amplitude: 2 }).easeOut) > peak(Easings.elastic().easeOut));
  // A given period replaces the default of every curve of the family.
  const slow = Easings.elastic({ period: 0.6 });
  assert.notEqual(slow.easeOut(0.3), Easings.easeOutElastic(0.3));
  assert.notEqual(slow.easeInOut(0.3), Easings.easeInOutElastic(0.3));
  // The general form is the named curves' own: their amplitude and period give them back.
  const named = Easings.elastic({ amplitude: 1, period: 0.3 });
  for (const t of sweep) {
    assert.ok(Math.abs(named.easeIn(t) - Easings.easeInElastic(t)) < 1e-12);
    assert.ok(Math.abs(named.easeOut(t) - Easings.easeOutElastic(t)) < 1e-12);
    assert.ok(Math.abs(named.easeInOut(t) - Easings.easeInOutElastic(t)) < 1e-12);
  }
  assert.throws(() => Easings.elastic({ amplitude: 0.5 }), /amplitude/);
  assert.throws(() => Easings.elastic({ period: 0 }), /period/);
});
