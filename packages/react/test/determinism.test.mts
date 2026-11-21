// While a frame is evaluated, `Math.random` is reseeded from the frame's
// time and the clock is frozen at it; calls from the entry are reported once.

import assert from 'node:assert/strict';
import { afterEach, beforeEach, test } from 'vitest';
import * as React from 'react';
import type { ReactElement } from 'react';

import { Composition, Text } from '../src/index.js';
import type { Time } from '../src/index.js';
import {
  evaluateDeterministically,
  frameSeed,
  resetNondeterministicCalls,
  setNondeterminismSource,
  takeNondeterministicCalls,
} from '../src/determinism.js';
import { createResolver, mount } from '../src/render.js';
import { registerComponent } from '../src/registry.js';

const h = React.createElement;
const at = (frame: number): Time => ({ value: frame, timescale: 30 });
const realRandom = Math.random;
const RealDate = Date;
const realPerformanceNow = performance.now;

// This file stands for the entry: its calls are the ones reported.
beforeEach(() => {
  resetNondeterministicCalls();
  setNondeterminismSource((frame) => frame.includes('determinism.test'));
});
afterEach(() => setNondeterminismSource(null));

/** A composition whose text is what `values()` returned while it rendered. */
function film(values: () => unknown[]) {
  function Values(): ReactElement {
    return h(Text, null, values().join(' '));
  }
  return () => h(Composition, { width: 8, height: 8, fps: 30, durationInFrames: 90 }, h(Values));
}

function textAt(mounted: ReturnType<typeof mount>, frame: number): string {
  const [layer] = mounted.renderAt(at(frame), null).scene.layers;
  assert.ok(layer && layer.content.type === 'text');
  return layer.content.text;
}

test('Math.random gives a frame the same numbers whatever order frames render in', () => {
  const entry = film(() => [Math.random(), Math.random(), Math.random()]);
  const forward = mount(entry);
  const backward = mount(entry);
  const frames = [0, 1, 29, 60];
  const seen = frames.map((frame) => textAt(forward, frame));
  for (const [index, frame] of [...frames.entries()].reverse()) {
    assert.equal(textAt(backward, frame), seen[index]);
  }
  // Again on the same root, after other frames.
  assert.equal(textAt(forward, 1), seen[1]);
  assert.equal(new Set(seen).size, frames.length, 'frames get their own numbers');
  const numbers = seen[0].split(' ').map(Number);
  assert.equal(new Set(numbers).size, 3, 'a sequence, not one number');
  assert.ok(numbers.every((value) => value >= 0 && value < 1));
});

test('the seed depends on the time, not on how its fraction is written', () => {
  assert.equal(frameSeed({ value: 1, timescale: 30 }), frameSeed({ value: 2, timescale: 60 }));
  assert.notEqual(frameSeed({ value: 1, timescale: 30 }), frameSeed({ value: 2, timescale: 30 }));
});

test('the clock reads the frame time', () => {
  const values = evaluateDeterministically({ value: 45, timescale: 30 }, () => ({
    now: Date.now(),
    performance: performance.now(),
    date: new Date().getTime(),
    string: Date(),
    explicit: new Date(86_400_000).getTime(),
    parsed: Date.parse('2000-01-01T00:00:00Z'),
    isDate: new Date() instanceof RealDate && new RealDate() instanceof Date,
  }));
  assert.equal(values.now, 1500);
  assert.equal(values.performance, 1500);
  assert.equal(values.date, 1500);
  assert.equal(values.string, new RealDate(1500).toString());
  // Dates of a given time and the static helpers are left alone.
  assert.equal(values.explicit, 86_400_000);
  assert.equal(values.parsed, 946_684_800_000);
  assert.ok(values.isDate);
  // A frame between two whole milliseconds: performance.now keeps the fraction.
  evaluateDeterministically({ value: 1, timescale: 30 }, () => {
    assert.equal(Date.now(), 33);
    assert.ok(Math.abs(performance.now() - 1000 / 30) < 1e-9);
  });
});

test('outside a frame the real functions are back', () => {
  evaluateDeterministically(at(3), () => {
    assert.notEqual(Math.random, realRandom);
    assert.notEqual(Date, RealDate);
  });
  assert.equal(Math.random, realRandom);
  assert.equal(Date, RealDate);
  assert.equal(performance.now, realPerformanceNow);
  assert.ok(Date.now() > 1e12);
  // Even when the frame throws.
  assert.throws(() => evaluateDeterministically(at(3), () => {
    throw new Error('boom');
  }), /boom/);
  assert.equal(Math.random, realRandom);
});

test('a nested evaluation starts afresh and leaves the outer sequence where it was', () => {
  const plain = evaluateDeterministically(at(5), () => [Math.random(), Math.random()]);
  const nested = evaluateDeterministically(at(5), () => {
    const first = Math.random();
    const inner = evaluateDeterministically(at(5), () => Math.random());
    return { first, inner, second: Math.random() };
  });
  assert.deepEqual([nested.first, nested.second], plain);
  assert.equal(nested.inner, plain[0]);
});

test("the entry's calls are reported once each, in the order they were first made", () => {
  const mounted = mount(film(() => [Math.random(), Date.now()]));
  // Mounting renders the entry already.
  assert.deepEqual(takeNondeterministicCalls(), ['Math.random()', 'Date.now()']);
  mounted.renderAt(at(10), null);
  assert.deepEqual(takeNondeterministicCalls(), []);
  evaluateDeterministically(at(1), () => [performance.now(), new Date(), [0].map(Math.random)]);
  assert.deepEqual(takeNondeterministicCalls(), ['performance.now()', 'new Date()']);
});

test('calls from other code, React included, are not reported', () => {
  const mounted = mount(film(() => ['still']));
  mounted.renderAt(at(10), null);
  setNondeterminismSource(() => false);
  evaluateDeterministically(at(1), () => Math.random());
  assert.deepEqual(takeNondeterministicCalls(), []);
});

test('resolved components render with the runtime time', () => {
  registerComponent('DeterminismProbe', () => h(Text, null, `${Date.now()} ${Math.random()}`));
  const resolve = (frame: number) => {
    const { components } = createResolver().resolve(
      [{ component: 'DeterminismProbe', props: {} }],
      { width: 8, height: 8, fps: 30, durationInFrames: 90, time: at(frame) },
    );
    const [text] = components[0] ?? [];
    assert.ok(text && text.content.type === 'text');
    return text.content.text;
  };
  const later = resolve(60);
  assert.ok(later.startsWith('2000 '));
  assert.equal(resolve(60), later);
  assert.notEqual(resolve(30), later);
});
