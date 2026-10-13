import assert from 'node:assert/strict';
import test from 'node:test';
import { SCRUB_TURNS, scrubFrame } from '../shared/scrub.ts';

test('scrub revisits frames, so the ball must repeat', () => {
  const seen = new Map<number, number>();
  for (let l = 0; l < 240; l += 1) seen.set(scrubFrame(l), (seen.get(scrubFrame(l)) ?? 0) + 1);
  assert.ok([...seen.values()].some((n) => n >= 3));
  for (let l = 0; l < 240; l += 1) assert.ok(Number.isInteger(scrubFrame(l)));
});

test('the scrub sweeps 0, 89, 30, 120, 60, turning where the score plucks', () => {
  assert.deepEqual(SCRUB_TURNS, [60, 110, 170]);
  assert.deepEqual([0, 60, 110, 170, 239].map(scrubFrame), [0, 89, 30, 120, 60]);
  assert.ok(scrubFrame(85) < scrubFrame(84)); // moving backwards between the first two turns
});
