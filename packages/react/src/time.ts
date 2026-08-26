// Shared exact-time helpers. `Time` is the generated `{ value, timescale }`
// rational from mikan_composition; seconds are converted at a microsecond
// timescale — comfortably finer than any frame rate or audio clock — so
// trimming and sequence offsets are not visibly quantized.

import type { Time } from './scene';

export const SECONDS_TIMESCALE = 1_000_000;

export function secondsToTime(seconds: number): Time {
  return { value: Math.round(seconds * SECONDS_TIMESCALE), timescale: SECONDS_TIMESCALE };
}

export function secondsFromTime(time: Time): number {
  return time.value / time.timescale;
}
