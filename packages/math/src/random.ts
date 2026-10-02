// Deterministic randomness. Every frame must be a pure function of the frame
// number, so `Math.random()` is off limits in a composition; these return the
// same value for the same seed on every render, in preview and in export.
import { UINT32_RANGE, mix32, requireFinite, seedToInt, streamValue } from './hash';
import type { Seed } from './hash';
import type { Vec2 } from './vector';

/**
 * A pseudo-random number in `[0, 1)` for `seed`: the same seed always gives
 * the same number. Use an index, or a string such as `` `star-${i}-x` ``, so
 * that different properties get independent values.
 */
export function random(seed: Seed): number {
  return mix32(seedToInt(seed, 'random') ^ 0x9e3779b9) / UINT32_RANGE;
}

/** A pseudo-random number in `[min, max)` for `seed`. */
export function randomRange(seed: Seed, min: number, max: number): number {
  return min + (max - min) * random(seed);
}

/** A pseudo-random whole number from `min` to `max`, both included, for `seed`. */
export function randomInt(seed: Seed, min: number, max: number): number {
  requireFinite('randomInt', { min, max });
  const low = Math.ceil(min);
  const high = Math.floor(max);
  if (high < low) {
    throw new Error(`randomInt() has no whole number between ${min} and ${max}`);
  }
  return low + Math.floor(random(seed) * (high - low + 1));
}

/** `true` with the given `probability` (0.5 by default) for `seed`. */
export function randomBool(seed: Seed, probability = 0.5): boolean {
  return random(seed) < probability;
}

/** `-1` or `1`, each half the time, for `seed`. */
export function randomSign(seed: Seed): -1 | 1 {
  return random(seed) < 0.5 ? -1 : 1;
}

/** One element of `items`, picked for `seed`. */
export function randomPick<T>(seed: Seed, items: readonly T[]): T {
  if (items.length === 0) {
    throw new Error('randomPick() requires at least one item');
  }
  return items[Math.floor(random(seed) * items.length)];
}

/** A copy of `items` in a pseudo-random order for `seed` (Fisher–Yates). */
export function shuffle<T>(seed: Seed, items: readonly T[]): T[] {
  const base = seedToInt(seed, 'shuffle') ^ 0x5bd1e995;
  const result = items.slice();
  for (let i = result.length - 1; i > 0; i -= 1) {
    const j = Math.floor(streamValue(base, i) * (i + 1));
    [result[i], result[j]] = [result[j], result[i]];
  }
  return result;
}

/**
 * A normally distributed pseudo-random number for `seed`: most values land
 * within `stdDev` of `mean`, a few much further (Box–Muller).
 */
export function randomGaussian(seed: Seed, mean = 0, stdDev = 1): number {
  const base = seedToInt(seed, 'randomGaussian') ^ 0x27d4eb2f;
  // 1 - u keeps the logarithm's argument in (0, 1].
  const u = 1 - streamValue(base, 0);
  const v = streamValue(base, 1);
  return mean + stdDev * Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
}

/** A pseudo-random point spread evenly over a disc of `radius` about `center`, for `seed`. */
export function randomInCircle(seed: Seed, radius = 1, center: Vec2 = { x: 0, y: 0 }): Vec2 {
  const base = seedToInt(seed, 'randomInCircle') ^ 0x165667b1;
  // The square root spreads points evenly by area instead of crowding the center.
  const r = radius * Math.sqrt(streamValue(base, 0));
  const angle = 2 * Math.PI * streamValue(base, 1);
  return { x: center.x + r * Math.cos(angle), y: center.y + r * Math.sin(angle) };
}
