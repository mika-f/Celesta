// Shared exact-time helpers. `Time` is the generated `{ value, timescale }`
// rational from celesta_composition; seconds are converted at a microsecond
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

/** Converts a `HH:MM:SS(.mmm)`, `MM:SS(.mmm)`, or `SS(.mmm)` timecode to a frame. */
export function timecodeToFrame(timecode: string, fps: number): number {
  const trimmed = timecode.trim();
  if (!/^\d+(?::\d+){0,2}(?:\.\d{1,3})?$/.test(trimmed)) {
    throw new Error(`Invalid timecode: ${timecode}`);
  }
  if (!Number.isFinite(fps) || fps <= 0) {
    throw new Error('timecodeToFrame() requires a positive fps');
  }

  const seconds = trimmed
    .split(':')
    .reduce((total, component) => total * 60 + Number(component), 0);
  return Math.round(seconds * fps);
}
