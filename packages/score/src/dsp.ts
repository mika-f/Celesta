// The building blocks the voices are made of. Everything is plain arithmetic
// over `dmath`, so a voice gives the same samples on every system.

import { PI, cos, exp, sin, sinCycles } from './dmath.js';

/** The rate every score renders at. */
export const SAMPLE_RATE = 48000;

/** Per-sample multiplier of an exponential decay with time constant `tau` seconds. */
export function decayFactor(tau: number): number {
  return exp(-1 / (tau * SAMPLE_RATE));
}

/** A buffer of `seconds` of silence. */
export function buffer(seconds: number): Float64Array {
  return new Float64Array(Math.max(1, Math.round(seconds * SAMPLE_RATE)));
}

/** White noise in [-1, 1). */
export function noise(rnd: () => number): number {
  return 2 * rnd() - 1;
}

/** A state-variable filter (Simper's trapezoidal form): low, band and high at once. */
export class Svf {
  low = 0;
  band = 0;
  high = 0;
  private ic1 = 0;
  private ic2 = 0;
  private a1 = 0;
  private a2 = 0;
  private a3 = 0;
  private k = 1;

  constructor(cutoff: number, q: number) {
    this.set(cutoff, q);
  }

  set(cutoff: number, q: number): void {
    const fc = Math.min(Math.max(cutoff, 10), SAMPLE_RATE * 0.45);
    const w = (PI * fc) / SAMPLE_RATE;
    const g = sin(w) / cos(w);
    this.k = 1 / q;
    this.a1 = 1 / (1 + g * (g + this.k));
    this.a2 = g * this.a1;
    this.a3 = g * this.a2;
  }

  process(v0: number): void {
    const v3 = v0 - this.ic2;
    const v1 = this.a1 * this.ic1 + this.a2 * v3;
    const v2 = this.ic2 + this.a2 * this.ic1 + this.a3 * v3;
    this.ic1 = 2 * v1 - this.ic1;
    this.ic2 = 2 * v2 - this.ic2;
    this.low = v2;
    this.band = v1;
    this.high = v0 - this.k * v1 - v2;
  }
}

/** A one-pole lowpass. */
export class OnePole {
  private y = 0;
  private a: number;

  constructor(cutoff: number) {
    this.a = 1 - exp((-2 * PI * Math.min(cutoff, SAMPLE_RATE * 0.45)) / SAMPLE_RATE);
  }

  process(x: number): number {
    this.y += this.a * (x - this.y);
    return this.y;
  }
}

/** A band-limited sawtooth (PolyBLEP) in [-1, 1]. */
export class Saw {
  private phase: number;

  constructor(phase = 0) {
    this.phase = phase;
  }

  next(freq: number): number {
    const dt = freq / SAMPLE_RATE;
    const t = this.phase;
    let value = 2 * t - 1;
    if (t < dt) {
      const x = t / dt;
      value -= x + x - x * x - 1;
    } else if (t > 1 - dt) {
      const x = (t - 1) / dt;
      value -= x * x + x + x + 1;
    }
    this.phase += dt;
    if (this.phase >= 1) this.phase -= 1;
    return value;
  }
}

/** A sine whose frequency may change every sample. */
export class Sine {
  private phase: number;

  constructor(phase = 0) {
    this.phase = phase;
  }

  next(freq: number): number {
    const value = sinCycles(this.phase);
    this.phase += freq / SAMPLE_RATE;
    if (this.phase >= 1) this.phase -= Math.floor(this.phase);
    return value;
  }
}

/** One decaying sine partial: frequency ratio, amplitude, time constant in seconds. */
export type Partial = readonly [ratio: number, amplitude: number, tau: number];

/** Adds decaying sine partials of `freq` into `out` from `offset`. Partials above Nyquist are skipped. */
export function addPartials(out: Float64Array, freq: number, partials: readonly Partial[], gain: number, offset = 0): void {
  for (const [ratio, amplitude, tau] of partials) {
    const f = freq * ratio;
    if (f >= SAMPLE_RATE * 0.45 || amplitude === 0) continue;
    const step = f / SAMPLE_RATE;
    const k = decayFactor(tau);
    let env = amplitude * gain;
    let phase = 0;
    for (let i = offset; i < out.length; i += 1) {
      out[i] += env * sinCycles(phase);
      phase += step;
      if (phase >= 1) phase -= 1;
      env *= k;
      if (env < 1e-7) break;
    }
  }
}

/**
 * Shapes a held note: after `hold` samples it falls away with time constant
 * `release`; the first `attack` samples rise linearly. The last 2 ms always
 * fade to zero, so a buffer never ends on a click.
 */
export function shape(out: Float64Array, hold: number, release: number, attack = 24): void {
  const n = out.length;
  for (let i = 0; i < Math.min(attack, n); i += 1) out[i] *= i / attack;
  if (hold < n) {
    const k = decayFactor(release);
    let env = 1;
    for (let i = Math.max(0, hold); i < n; i += 1) {
      env *= k;
      out[i] *= env;
    }
  }
  const tail = Math.min(96, n);
  for (let i = 0; i < tail; i += 1) out[n - 1 - i] *= i / tail;
}

/** Sound past this many seconds after a held note ends is cut. */
export function heldLength(hold: number, release: number, natural: number): number {
  return Math.min(natural, hold / SAMPLE_RATE + release * 7);
}
