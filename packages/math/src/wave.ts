// Periodic waves in `[-1, 1]` with a period of 1: pass `frame / framesPerCycle`.
// All four start at 0 and rise, so they can be swapped for one another.
import { fract } from './scalar';

/** A sine wave: `sin(2πt)`. */
export function sineWave(t: number): number {
  return Math.sin(2 * Math.PI * t);
}

/** A triangle wave: straight ramps up to 1 at `t = 0.25` and down to -1 at `t = 0.75`. */
export function triangleWave(t: number): number {
  return 1 - 4 * Math.abs(fract(t + 0.25) - 0.5);
}

/** A square wave: 1 for the first half of each period, -1 for the second. */
export function squareWave(t: number): number {
  return fract(t) < 0.5 ? 1 : -1;
}

/** A sawtooth wave: ramps from 0 up to 1, drops to -1 at `t = 0.5`, and ramps back to 0. */
export function sawtoothWave(t: number): number {
  return 2 * fract(t + 0.5) - 1;
}
