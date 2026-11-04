import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'vitest';

import type { AnimatedNumber, AudioDuck } from '@celesta.studio/react';
import { checkDuckGroup, duckEnvelope, duckGroups } from '../src/duck.js';

interface Fixture {
  cases: { name: string; intervals: [number, number][]; duck: Omit<AudioDuck, 'under'>; curve: AnimatedNumber }[];
}

// The cases `crates/composition/tests/duck_fixture.rs` pins `duck_curve` to,
// so the browser and the native mixer duck alike.
const fixture: Fixture = JSON.parse(readFileSync(new URL('../../../crates/composition/tests/fixtures/duck-curves.json', import.meta.url), 'utf8'));

function assertCurve(actual: AnimatedNumber, expected: AnimatedNumber, name: string) {
  if (typeof expected === 'number' || typeof actual === 'number') {
    assert.equal(actual, expected, name);
    return;
  }
  assert.equal(actual.keyframes.length, expected.keyframes.length, name);
  actual.keyframes.forEach((key, index) => {
    const want = expected.keyframes[index];
    const at = `${name} key ${index}`;
    assert.deepEqual(key.time, want.time, at);
    assert.ok(Math.abs(key.value - want.value) <= 1e-12, `${at}: ${key.value}`);
    assert.equal(key.easing ?? undefined, want.easing ?? undefined, at);
  });
}

test('every fixture curve matches the native one', () => {
  for (const { name, intervals, duck, curve } of fixture.cases) {
    assertCurve(duckEnvelope(intervals, duck), curve, name);
  }
});

test('origin shifts the keys onto the sequence clock', () => {
  const curve = duckEnvelope([[2, 4]], {}, { origin: 1.5 });
  assert.ok(typeof curve !== 'number');
  assert.deepEqual(curve.keyframes[0].time, { value: 350_000, timescale: 1_000_000 });
  assert.deepEqual(curve.keyframes[3].time, { value: 2_900_000, timescale: 1_000_000 });
});

test('volume scales the curve', () => {
  const curve = duckEnvelope([[2, 4]], { volume: 0.8, to: 0.3 });
  assert.ok(typeof curve !== 'number');
  assert.deepEqual(curve.keyframes.map((key) => key.value), [0.8, 0.8 * 0.3, 0.8 * 0.3, 0.8]);
  assert.equal(duckEnvelope([], { volume: 0.8 }), 0.8);
});

test('invalid values throw with the field', () => {
  assert.throws(() => duckEnvelope([], { to: 1.5 }), /duck\.to must be a multiplier from 0 to 1, or \{ db \} at or below 0/);
  assert.throws(() => duckEnvelope([], { to: { db: 3 } }), /duck\.to/);
  assert.throws(() => duckEnvelope([], { attack: -1 }), /duck\.attack must be finite and not negative/);
  assert.throws(() => duckEnvelope([], { holdGap: Number.NaN }), /duck\.holdGap/);
  assert.throws(() => duckEnvelope([], { volume: -1 }), /volume must be finite and not negative/);
});

test('groups must be non-empty names', () => {
  checkDuckGroup('voice');
  assert.throws(() => checkDuckGroup(''), /^Error: duckGroup must be a non-empty name$/);
  assert.deepEqual(duckGroups('voice'), ['voice']);
  assert.deepEqual(duckGroups(['voice', 'sfx']), ['voice', 'sfx']);
  assert.throws(() => duckGroups([]), /^Error: duck\.under must be a non-empty name$/);
  assert.throws(() => duckGroups(['voice', '']), /^Error: duck\.under must be a non-empty name$/);
});
