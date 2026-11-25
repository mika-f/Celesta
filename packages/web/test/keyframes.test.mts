import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'vitest';
import { evaluateAnimated, integrateAnimated } from '@celesta.studio/react/internal/animated-number';
import { MAX_POINTS_PER_SEGMENT, automationEvents } from '../src/keyframes.js';
import type { AutomationEvent } from '../src/keyframes.js';
import type { AnimatedNumber, Keyframe, KeyframeEasing } from '../src/types.js';

interface FixtureSample { time: Keyframe['time']; value: number; integral: number }
interface Fixture { cases: { name: string; animation: AnimatedNumber; samples: FixtureSample[] }[] }

// The same cases `crates/composition/tests/keyframe_fixture.rs` pins the
// native evaluator to, so the two cannot drift apart.
const fixture: Fixture = JSON.parse(readFileSync(new URL('../../../crates/composition/tests/fixtures/keyframe-evaluation.json', import.meta.url), 'utf8'));

test('every fixture sample matches the Rust evaluator', () => {
  for (const { name, animation, samples } of fixture.cases) {
    for (const { time, value, integral } of samples) {
      const seconds = time.value / time.timescale;
      const at = `${name} @ ${seconds}`;
      assert.ok(Math.abs(evaluateAnimated(animation, seconds) - value) <= 1e-9, `${at}: value ${evaluateAnimated(animation, seconds)}, expected ${value}`);
      assert.ok(Math.abs(integrateAnimated(animation, seconds) - integral) <= 1e-9, `${at}: integral ${integrateAnimated(animation, seconds)}, expected ${integral}`);
    }
  }
});
test('empty keyframes throw', () => {
  assert.throws(() => evaluateAnimated({ type: 'keyframes', keyframes: [] }, 0), /animation has no keyframes/);
  assert.throws(() => integrateAnimated({ type: 'keyframes', keyframes: [] }, 0), /animation has no keyframes/);
});
test('an unknown easing throws', () => {
  // Deliberately outside KeyframeEasing, as untyped JSON can be.
  const wobble = 'wobble' as KeyframeEasing;
  const animation: AnimatedNumber = { type: 'keyframes', keyframes: [key(0, 0), key(1, 1, wobble)] };
  assert.throws(() => evaluateAnimated(animation, 0.5), /unknown easing: wobble/);
});

function key(time: number, value: number, easing?: KeyframeEasing): Keyframe {
  return { time: { value: Math.round(time * 1_000_000), timescale: 1_000_000 }, value, ...(easing ? { easing } : {}) };
}
function keys(...list: [time: number, value: number, easing?: KeyframeEasing][]): AnimatedNumber {
  return { type: 'keyframes', keyframes: list.map(([time, value, easing]) => key(time, value, easing)) };
}
const set = (value: number, time: number): AutomationEvent => ({ kind: 'set', value, time });
const ramp = (value: number, time: number): AutomationEvent => ({ kind: 'ramp', value, time });

test('a static value is one set at the clip start', () => {
  assert.deepEqual(automationEvents(0.5, 2, 4), [set(0.5, 2)]);
});
test('linear keys become one ramp each', () => {
  assert.deepEqual(automationEvents(keys([0, 0], [1, 2, 'linear']), 3, 10), [set(0, 3), ramp(2, 4)]);
});
test('an eased segment is sampled every millisecond', () => {
  const volume = keys([0, 1], [0.3, 0.3, 'ease-in-out']);
  const events = automationEvents(volume, 0, 1);
  assert.equal(events.length, 301);
  assert.deepEqual(events[0], set(1, 0));
  for (const event of events.slice(1)) {
    assert.equal(event.kind, 'ramp');
    assert.ok(Math.abs(event.value - evaluateAnimated(volume, event.time)) <= 1e-12);
  }
  const last = events[events.length - 1];
  assert.ok(Math.abs(last.time - 0.3) <= 1e-12);
  assert.equal(last.value, 0.3);
});
test('a cut is a set at the same time', () => {
  const events = automationEvents(keys([0, 0], [1, 1, 'linear'], [1, 3], [2, 3]), 0, 2);
  assert.deepEqual(events.slice(0, 3), [set(0, 0), ramp(1, 1), set(3, 1)]);
});
test('a clip starting before 0 starts from where the curve has got to', () => {
  assert.deepEqual(automationEvents(keys([0, 0], [2, 2, 'linear']), -1, 5), [set(1, 0), ramp(2, 1)]);
});
test('a long eased segment is capped', () => {
  assert.equal(automationEvents(keys([0, 0], [600, 1, 'ease-in-out-sine']), 0, 600).length, 1 + MAX_POINTS_PER_SEGMENT);
});
test('a cut at the clip start takes effect from the start', () => {
  assert.deepEqual(automationEvents(keys([0, 0], [0, 1], [1, 2, 'linear']), 0, 2), [set(0, 0), set(1, 0), ramp(2, 1)]);
  // An outer sequence clipping the head moves a cut onto local 0.
  assert.deepEqual(automationEvents(keys([-1, 1], [0, 1], [0, 0.2], [1, 0.2]), 0, 2), [set(1, 0), set(0.2, 0), ramp(0.2, 1)]);
});
test('windows past the end are not scheduled', () => {
  const events = automationEvents(keys([0, 0], [1, 1, 'linear'], [5, 0, 'linear'], [6, 1, 'linear']), 0, 2);
  assert.deepEqual(events, [set(0, 0), ramp(1, 1), ramp(0.75, 2)]);
});
test('an eased window across the end is followed only up to the end', () => {
  const rate = keys([0, 0], [600, 1, 'ease-in-out-sine']);
  const events = automationEvents(rate, 0, 0.5);
  assert.equal(events.length, 1 + 500);
  assert.equal(events[events.length - 1].time, 0.5);
  assert.equal(events[events.length - 1].value, evaluateAnimated(rate, 0.5));
});
test('values are clamped to the range', () => {
  // ease-in-back dips to about -0.09 before rising.
  const events = automationEvents(keys([0, 0], [1, 1, 'ease-in-back']), 0, 1, { min: 0, max: 16 });
  assert.ok(events.some((event) => event.value === 0));
  assert.ok(events.every((event) => event.value >= 0 && event.value <= 16));
});
