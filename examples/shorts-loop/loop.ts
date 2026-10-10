import { DURATION } from './constants';

// Clocks that come back to where they started after DURATION frames, so the
// last frame runs straight into the first. Everything that moves is built
// from these.

export const TAU = Math.PI * 2;

/** The loop's angle: 0 at frame 0, approaching 2π at the last frame. */
export const theta = (frame: number) => (TAU * frame) / DURATION;

/**
 * A sine that turns `cycles` times per loop. Only whole numbers come back to
 * their start, so anything else throws rather than leaving a seam.
 */
export function wave(frame: number, cycles: number, phase = 0) {
  if (!Number.isInteger(cycles)) throw new Error(`wave: ${cycles} cycles per loop would not loop`);
  return Math.sin(cycles * theta(frame) + phase);
}

/** Where `frame` sits in a span of `length` frames (a beat, a bar), 0–1. */
export function within(frame: number, length: number) {
  if (DURATION % length !== 0) throw new Error(`within: ${length} frames does not divide the loop`);
  return (((frame % length) + length) % length) / length;
}

/** A hit: 1 on the first frame of every `length`, decaying before the next. */
export const hit = (frame: number, length: number, decay = 5) => Math.exp(-decay * within(frame, length));
