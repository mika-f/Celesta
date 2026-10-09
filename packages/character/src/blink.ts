// Portrait blinking. Whether a character's eyes are shut on a frame is a pure
// function of the composition frame and a seed, so preview and export agree
// and every frame can be rendered on its own, in any order.

import { random } from '@celesta/math';

/** When a portrait blinks; see `blinkPhase()`. */
export interface BlinkTiming {
  /** Average seconds from one blink to the next. Each gap varies by ±50%. Default 4. */
  interval?: number;
  /** Seconds the eyes stay fully shut, at least one frame. Default 0.1. */
  duration?: number;
  /**
   * Picks the rhythm: the same seed always blinks on the same frames, and
   * different seeds blink independently. A portrait defaults to its
   * character's id, so two characters never blink in lockstep by accident.
   */
  seed?: number | string;
}

/** The state of the eyes on one frame. `'half'` frames bracket every blink. */
export type BlinkPhase = 'open' | 'half' | 'closed';

const DEFAULT_INTERVAL = 4;
const DEFAULT_DURATION = 0.1;
/** How far, as a fraction of `interval`, a blink may drift from its slot. */
const JITTER = 0.5;

function blinkStart(slot: number, intervalFrames: number, seed: number | string): number {
  const offset = (random(`${seed}:blink:${slot}`) - 0.5) * JITTER;
  return Math.round((slot + 0.5 + offset) * intervalFrames);
}

/**
 * The eyes' state on `frame` (a composition frame) at `fps`. Time is cut into
 * `interval`-long slots with one blink in each, placed at a seeded offset
 * around the slot's middle, so gaps run from half to one and a half
 * `interval`s and never pile up. The eyes are shut for `duration`, with a
 * `'half'` frame (two at 60 fps) on either side; portraits without half-shut
 * eyes show them open there.
 */
export function blinkPhase(frame: number, fps: number, timing: BlinkTiming = {}): BlinkPhase {
  const interval = timing.interval ?? DEFAULT_INTERVAL;
  const duration = timing.duration ?? DEFAULT_DURATION;
  const seed = timing.seed ?? 0;
  if (!Number.isFinite(frame)) {
    throw new Error('blinkPhase() requires a finite frame');
  }
  if (!Number.isFinite(fps) || fps <= 0) {
    throw new Error('blinkPhase() requires a positive fps');
  }
  if (!Number.isFinite(interval) || interval <= 0) {
    throw new Error('blink interval must be a positive number of seconds');
  }
  if (!Number.isFinite(duration) || duration <= 0) {
    throw new Error('blink duration must be a positive number of seconds');
  }
  const intervalFrames = interval * fps;
  const shutFrames = Math.max(1, Math.round(duration * fps));
  const halfFrames = Math.max(1, Math.round(fps * 0.04));
  const slot = Math.floor(frame / intervalFrames);
  // A blink stays near its own slot, so only the neighbours can reach `frame`.
  let phase: BlinkPhase = 'open';
  for (let k = slot - 1; k <= slot + 1; k += 1) {
    const start = blinkStart(k, intervalFrames, seed);
    if (frame >= start && frame < start + shutFrames) {
      return 'closed';
    }
    if (frame >= start - halfFrames && frame < start + shutFrames + halfFrames) {
      phase = 'half';
    }
  }
  return phase;
}
