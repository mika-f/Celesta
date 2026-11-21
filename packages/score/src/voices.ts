// The built-in sounds: melodic patches, drums, and effects. Each renders one
// event into a mono buffer at unit velocity; the mixer applies velocity,
// gain, pan, and the reverb send. Levels are set so a hit at velocity 1
// peaks around 0.5 (drums and effects) or a note around 0.35 (patches).

import { TAU, semitoneRatio, sin, sinCycles } from './dmath.js';
import {
  OnePole,
  SAMPLE_RATE,
  Saw,
  Sine,
  Svf,
  addPartials,
  buffer,
  decayFactor,
  heldLength,
  noise,
  shape,
} from './dsp.js';
import type { Partial } from './dsp.js';

const SR = SAMPLE_RATE;

export interface PatchInput {
  /** Hz. */
  freq: number;
  /** Samples the note is held. */
  hold: number;
  /** 0 (dull) to 1 (bright). */
  bright: number;
  rnd: () => number;
}

export interface HitInput {
  /** Frequency ratio from `pitch` (1 = as designed). */
  ratio: number;
  /** Seconds the effect lasts, for the effects that take a length. */
  dur: number | undefined;
  rnd: () => number;
}

/** A rendered sound and the sample in it that lands on the event's time. */
export interface Rendered {
  data: Float64Array;
  /** 0 for sounds that start on their time; the end for those that arrive at it. */
  anchor: number;
}

// ---------------------------------------------------------------- patches

function mallet(input: PatchInput, partials: readonly Partial[], release: number, natural: number, click: number): Float64Array {
  const { freq, hold, rnd } = input;
  const out = buffer(heldLength(hold, release, natural));
  addPartials(out, freq, partials, 0.35);
  // The mallet's knock: 4 ms of filtered noise.
  const knock = new Svf(Math.min(freq * 6, 9000), 0.8);
  const k = decayFactor(0.0012);
  let env = click;
  for (let i = 0; i < Math.min(out.length, 200); i += 1) {
    knock.process(noise(rnd));
    out[i] += env * knock.band;
    env *= k;
  }
  shape(out, hold, release, 8);
  return out;
}

function marimba(input: PatchInput): Float64Array {
  const { freq, bright } = input;
  const tau = Math.min(0.7, Math.max(0.12, 90 / freq));
  return mallet(input, [[1, 1, tau], [3.93, 0.35 * bright, tau / 5], [9.2, 0.08 * bright, tau / 12]], 0.06, tau * 7, 0.12);
}

function vibes(input: PatchInput): Float64Array {
  const { bright } = input;
  const out = mallet(input, [[1, 1, 1.6], [4, 0.22 * bright, 0.5], [10, 0.05 * bright, 0.12]], 0.18, 6, 0.06);
  // The motor's tremolo, 5.5 Hz.
  for (let i = 0; i < out.length; i += 1) out[i] *= 1 - 0.22 * (0.5 + 0.5 * sinCycles((5.5 * i) / SR));
  return out;
}

function glock(input: PatchInput): Float64Array {
  const { bright } = input;
  return mallet(input, [[1, 1, 1.4], [2.76, 0.5 * bright, 0.5], [5.4, 0.25 * bright, 0.2], [8.93, 0.12 * bright, 0.09]], 0.25, 5, 0.04);
}

function ep(input: PatchInput): Float64Array {
  const { freq, hold, bright } = input;
  const out = buffer(heldLength(hold, 0.14, 5));
  // Phase modulation: a 1:1 modulator for the body, a 14:1 one for the tine.
  const kAmp = decayFactor(1.4);
  const kIndex = decayFactor(0.35);
  const kTine = decayFactor(0.025);
  const step = freq / SR;
  let carrier = 0;
  let amp = 0.32;
  let index = 0.4 + 1.6 * bright;
  let tine = 0.6 * bright;
  for (let i = 0; i < out.length; i += 1) {
    const body = (index + 0.15) * sinCycles(carrier);
    const bell = tine * sinCycles(14 * carrier);
    out[i] = amp * sin(TAU * carrier + body + bell);
    carrier += step;
    if (carrier >= 1) carrier -= 1;
    amp *= kAmp;
    index *= kIndex;
    tine *= kTine;
  }
  shape(out, hold, 0.14, 24);
  return out;
}

function keys(input: PatchInput): Float64Array {
  const { freq, hold, bright, rnd } = input;
  const partials: Partial[] = [];
  for (let k = 1; k <= 8; k += 1) {
    const stretch = k * (1 + 0.0004 * k * k);
    const amp = (k <= 2 ? 1 : bright * 1.2) / k;
    partials.push([stretch, amp, 1.8 / (0.6 + 0.4 * k)]);
  }
  const out = buffer(heldLength(hold, 0.25, 6));
  addPartials(out, freq, partials, 0.22);
  const hammer = new OnePole(3000);
  const k = decayFactor(0.002);
  let env = 0.05;
  for (let i = 0; i < Math.min(out.length, 400); i += 1) {
    out[i] += env * hammer.process(noise(rnd));
    env *= k;
  }
  shape(out, hold, 0.25, 12);
  return out;
}

function pluck(input: PatchInput): Float64Array {
  const { freq, hold, bright, rnd } = input;
  const out = buffer(heldLength(hold, 0.12, 4));
  const period = Math.max(2, Math.round(SR / freq));
  const line = new Float64Array(period + 1);
  const excite = new OnePole(1500 + 9000 * bright);
  for (let i = 0; i < line.length; i += 1) line[i] = excite.process(noise(rnd));
  let mean = 0;
  for (let i = 0; i < line.length; i += 1) mean += line[i];
  mean /= line.length;
  for (let i = 0; i < line.length; i += 1) line[i] -= mean;
  const loss = 0.4985 + 0.0013 * bright;
  let read = 0;
  for (let i = 0; i < out.length; i += 1) {
    const a = line[read];
    const b = line[(read + 1) % line.length];
    const y = (a + b) * loss;
    out[i] = 0.5 * a;
    line[read] = y;
    read = (read + 1) % line.length;
  }
  shape(out, hold, 0.12, 4);
  return out;
}

function bass(input: PatchInput): Float64Array {
  const { freq, hold, bright } = input;
  const out = buffer(heldLength(hold, 0.06, 4));
  const sine = new Sine();
  const saw = new Saw();
  const filter = new Svf(freq * 4, 0.9);
  // The filter closes every 32 samples, with a time constant of 0.18 s.
  const kFilter = decayFactor(0.18 / 32);
  const kAmp = decayFactor(1.2);
  let open = freq * (2 + 8 * bright);
  let amp = 0.7;
  for (let i = 0; i < out.length; i += 1) {
    if ((i & 31) === 0) {
      filter.set(freq * 1.5 + open, 0.9);
      open *= kFilter;
    }
    filter.process(saw.next(freq));
    out[i] = amp * (0.7 * sine.next(freq) + 0.5 * filter.low);
    amp *= kAmp;
  }
  shape(out, hold, 0.06, 48);
  return out;
}

function pad(input: PatchInput): Float64Array {
  const { freq, hold, bright } = input;
  const release = 0.6;
  const out = buffer(hold / SR + release * 6);
  const detune = [semitoneRatio(-0.08), 1, semitoneRatio(0.07)];
  const saws = [new Saw(0), new Saw(0.33), new Saw(0.71)];
  const sub = new Sine();
  const lp1 = new OnePole(freq * (2.5 + 5 * bright));
  const lp2 = new OnePole(freq * (2.5 + 5 * bright));
  const attack = Math.round(0.35 * SR);
  for (let i = 0; i < out.length; i += 1) {
    let sum = 0;
    for (let v = 0; v < 3; v += 1) sum += saws[v].next(freq * detune[v]);
    const x = lp2.process(lp1.process(sum / 3)) + 0.25 * sub.next(freq / 2);
    const rise = i < attack ? i / attack : 1;
    out[i] = 0.4 * x * rise * rise;
  }
  shape(out, hold, release, 1);
  return out;
}

export const PATCHES = {
  marimba,
  vibes,
  glock,
  ep,
  keys,
  pluck,
  bass,
  pad,
} as const satisfies Record<string, (input: PatchInput) => Float64Array>;

export type PatchName = keyof typeof PATCHES;

// ---------------------------------------------------------------- drums

function sweep(seconds: number, from: number, to: number, tauPitch: number, tauAmp: number, level: number): Float64Array {
  const out = buffer(seconds);
  const osc = new Sine();
  const kPitch = decayFactor(tauPitch);
  const kAmp = decayFactor(tauAmp);
  let excess = from - to;
  let amp = level;
  for (let i = 0; i < out.length; i += 1) {
    out[i] = amp * osc.next(to + excess);
    excess *= kPitch;
    amp *= kAmp;
  }
  return out;
}

function noiseHit(
  out: Float64Array,
  rnd: () => number,
  mode: 'low' | 'band' | 'high',
  cutoff: number,
  q: number,
  tau: number,
  level: number,
  offset = 0,
): void {
  const filter = new Svf(cutoff, q);
  const k = decayFactor(tau);
  let env = level;
  for (let i = offset; i < out.length; i += 1) {
    filter.process(noise(rnd));
    out[i] += env * filter[mode];
    env *= k;
    if (env < 1e-6) break;
  }
}

function finish(out: Float64Array, attack = 4): Float64Array {
  shape(out, out.length, 1, attack);
  return out;
}

function kick({ ratio, rnd }: HitInput): Float64Array {
  const out = sweep(0.5, 160 * ratio, 46 * ratio, 0.032, 0.2, 0.75);
  noiseHit(out, rnd, 'low', 4000, 0.7, 0.002, 0.25);
  return finish(out, 2);
}

function snare({ ratio, rnd }: HitInput): Float64Array {
  const out = sweep(0.3, 330 * ratio, 185 * ratio, 0.02, 0.06, 0.18);
  noiseHit(out, rnd, 'high', 1300 * ratio, 0.7, 0.11, 0.33);
  return finish(out);
}

function clap({ ratio, rnd }: HitInput): Float64Array {
  const out = buffer(0.35);
  for (const at of [0, 0.011, 0.022]) noiseHit(out, rnd, 'band', 1150 * ratio, 1.4, 0.006, 0.9, Math.round(at * SR));
  noiseHit(out, rnd, 'band', 1150 * ratio, 1.2, 0.085, 0.8, Math.round(0.03 * SR));
  return finish(out, 2);
}

function sidestick({ ratio, rnd }: HitInput): Float64Array {
  const out = sweep(0.08, 1700 * ratio, 1500 * ratio, 0.004, 0.012, 0.22);
  noiseHit(out, rnd, 'band', 2200 * ratio, 2, 0.01, 0.66);
  return finish(out, 2);
}

function hat({ ratio, rnd }: HitInput): Float64Array {
  const out = buffer(0.1);
  noiseHit(out, rnd, 'high', 7500 * ratio, 0.8, 0.022, 0.45);
  return finish(out, 2);
}

function openhat({ ratio, rnd }: HitInput): Float64Array {
  const out = buffer(0.6);
  noiseHit(out, rnd, 'high', 7000 * ratio, 0.8, 0.2, 0.4);
  return finish(out, 2);
}

function shaker({ ratio, rnd }: HitInput): Float64Array {
  const out = buffer(0.12);
  const filter = new Svf(6500 * ratio, 1.2);
  const rise = Math.round(0.018 * SR);
  const k = decayFactor(0.03);
  let env = 0.5;
  for (let i = 0; i < out.length; i += 1) {
    filter.process(noise(rnd));
    const a = i < rise ? i / rise : 1;
    out[i] = env * a * filter.band;
    if (i >= rise) env *= k;
  }
  return finish(out, 1);
}

function tamb({ ratio, rnd }: HitInput): Float64Array {
  const out = buffer(0.3);
  for (const at of [0, 0.014, 0.03]) noiseHit(out, rnd, 'high', 8500 * ratio, 2.5, 0.05, 0.17, Math.round(at * SR));
  return finish(out, 2);
}

function woodblock({ ratio, rnd }: HitInput): Float64Array {
  const out = buffer(0.15);
  addPartials(out, 880 * ratio, [[1, 1, 0.035], [2.7, 0.25, 0.012]], 0.5);
  noiseHit(out, rnd, 'band', 2500 * ratio, 2, 0.003, 0.4);
  return finish(out, 2);
}

function cowbell({ ratio }: HitInput): Float64Array {
  const out = buffer(0.45);
  // Two squares, from their first odd harmonics.
  const square: Partial[] = [];
  for (const base of [540, 800]) {
    for (const h of [1, 3, 5, 7]) square.push([(base * h) / 540, 1 / h, 0.12]);
  }
  addPartials(out, 540 * ratio, square, 0.12);
  const filter = new Svf(800 * ratio, 1.6);
  for (let i = 0; i < out.length; i += 1) {
    filter.process(out[i]);
    out[i] = 0.4 * out[i] + 1.4 * filter.band;
  }
  return finish(out, 2);
}

function tom({ ratio, rnd }: HitInput): Float64Array {
  const out = sweep(0.6, 190 * ratio, 110 * ratio, 0.08, 0.22, 0.6);
  noiseHit(out, rnd, 'low', 1200, 0.7, 0.008, 0.2);
  return finish(out, 2);
}

function crash({ ratio, rnd }: HitInput): Float64Array {
  const out = buffer(2.6);
  noiseHit(out, rnd, 'high', 3200 * ratio, 0.6, 0.85, 0.35);
  noiseHit(out, rnd, 'band', 5200 * ratio, 1.2, 0.4, 0.25);
  return finish(out, 24);
}

function ride({ ratio, rnd }: HitInput): Float64Array {
  const out = buffer(2);
  noiseHit(out, rnd, 'high', 5200 * ratio, 0.8, 0.5, 0.16);
  addPartials(out, 510 * ratio, [[1, 0.6, 0.7], [1.48, 0.5, 0.6], [2.35, 0.4, 0.45], [3.1, 0.3, 0.3]], 0.09);
  return finish(out, 4);
}

export const DRUMS = {
  kick,
  snare,
  clap,
  sidestick,
  hat,
  openhat,
  shaker,
  tamb,
  woodblock,
  cowbell,
  tom,
  crash,
  ride,
} as const satisfies Record<string, (input: HitInput) => Float64Array>;

export type DrumName = keyof typeof DRUMS;

// ---------------------------------------------------------------- effects

function start(data: Float64Array): Rendered {
  return { data, anchor: 0 };
}

function tick({ ratio, rnd }: HitInput): Rendered {
  const out = buffer(0.03);
  addPartials(out, 3200 * ratio, [[1, 1, 0.004], [1.9, 0.4, 0.002]], 0.32);
  noiseHit(out, rnd, 'high', 6000, 0.8, 0.0015, 0.16);
  return start(finish(out, 1));
}

function clickSound(ratio: number, rnd: () => number, level: number, out: Float64Array, offset: number): void {
  const at = new Float64Array(out.length - offset);
  addPartials(at, 1800 * ratio, [[1, 1, 0.006]], 0.35);
  noiseHit(at, rnd, 'band', 2600 * ratio, 1.5, 0.004, 0.9);
  for (let i = 0; i < at.length; i += 1) out[offset + i] += level * at[i];
}

function click({ ratio, rnd }: HitInput): Rendered {
  const out = buffer(0.05);
  clickSound(ratio, rnd, 1, out, 0);
  return start(finish(out, 1));
}

function doubleclick({ ratio, rnd }: HitInput): Rendered {
  const out = buffer(0.13);
  clickSound(ratio, rnd, 1, out, 0);
  clickSound(ratio, rnd, 0.8, out, Math.round(0.075 * SR));
  return start(finish(out, 1));
}

function pop({ ratio }: HitInput): Rendered {
  const out = sweep(0.16, 950 * ratio, 300 * ratio, 0.02, 0.045, 0.55);
  return start(finish(out, 24));
}

function bell(out: Float64Array, freq: number, level: number, offset = 0): void {
  addPartials(out, freq, [[1, 1, 0.9], [2.76, 0.35, 0.35], [5.4, 0.15, 0.14], [8.93, 0.06, 0.07]], level, offset);
}

function ding({ ratio }: HitInput): Rendered {
  const out = buffer(2.2);
  bell(out, 1568 * ratio, 0.33);
  return start(finish(out, 2));
}

function chime({ ratio }: HitInput): Rendered {
  const out = buffer(2.4);
  [1319, 1661, 1976].forEach((freq, index) => bell(out, freq * ratio, 0.2, Math.round(index * 0.08 * SR)));
  return start(finish(out, 2));
}

function sparkle({ ratio, dur, rnd }: HitInput): Rendered {
  const length = dur ?? 0.6;
  const out = buffer(length + 0.2);
  const count = Math.max(4, Math.round(length * 18));
  for (let n = 0; n < count; n += 1) {
    const at = Math.round(rnd() * length * SR);
    const freq = (3000 + 5000 * rnd()) * ratio;
    const level = 0.12 + 0.18 * rnd();
    addPartials(out, freq, [[1, 1, 0.03], [2.1, 0.3, 0.012]], level, at);
  }
  return start(finish(out, 2));
}

/** Noise through a band that sweeps up while the level swells, then falls. */
function whooshBody(length: number, ratio: number, rnd: () => number, rising: boolean): Float64Array {
  const out = buffer(length);
  const n = out.length;
  const filter = new Svf(400, 1.4);
  const peak = rising ? 0.97 : 0.62;
  for (let i = 0; i < n; i += 1) {
    const x = i / n;
    if ((i & 31) === 0) filter.set((300 + 3200 * x * x) * ratio, 1.4);
    filter.process(noise(rnd));
    const env = x < peak ? sin((x / peak) * (TAU / 4)) : sin(((1 - x) / (1 - peak)) * (TAU / 4));
    out[i] = (rising ? 0.6 : 0.8) * env * env * filter.band;
  }
  return out;
}

function whoosh({ ratio, dur, rnd }: HitInput): Rendered {
  return start(finish(whooshBody(dur ?? 0.6, ratio, rnd, false), 1));
}

function whooshInto({ ratio, dur, rnd }: HitInput): Rendered {
  const out = finish(whooshBody(dur ?? 0.6, ratio, rnd, true), 1);
  return { data: out, anchor: out.length };
}

function revswell({ ratio, dur }: HitInput): Rendered {
  const length = dur ?? 1.2;
  // A bell, rendered forwards and then reversed: it swells into its time.
  const forward = buffer(length);
  bell(forward, 880 * ratio, 0.22);
  bell(forward, 1320 * ratio, 0.12);
  addPartials(forward, 440 * ratio, [[1, 1, 0.8]], 0.12);
  const out = new Float64Array(forward.length);
  for (let i = 0; i < out.length; i += 1) out[i] = forward[forward.length - 1 - i];
  shape(out, out.length, 1, 96);
  return { data: out, anchor: out.length };
}

function roll({ ratio, dur, rnd }: HitInput): Rendered {
  const length = dur ?? 1;
  const out = buffer(length + 0.3);
  const hits = Math.max(2, Math.round(length * 22));
  for (let n = 0; n < hits; n += 1) {
    const level = 0.25 + 0.75 * (n / (hits - 1));
    const hit = snare({ ratio, dur: undefined, rnd });
    const at = Math.round(((n * length) / hits) * SR);
    for (let i = 0; i < hit.length && at + i < out.length; i += 1) out[at + i] += 0.7 * level * hit[i];
  }
  return start(finish(out, 2));
}

function boing({ ratio }: HitInput): Rendered {
  const out = buffer(0.7);
  const osc = new Sine();
  const kAmp = decayFactor(0.22);
  const kWobble = decayFactor(0.2);
  let amp = 0.5;
  let wobble = 0.45;
  for (let i = 0; i < out.length; i += 1) {
    const t = i / SR;
    const glide = 1 + 0.6 * (t < 0.25 ? t / 0.25 : 1);
    out[i] = amp * osc.next(180 * ratio * glide * (1 + wobble * sinCycles(9 * t)));
    amp *= kAmp;
    wobble *= kWobble;
  }
  return start(finish(out, 24));
}

function thud({ ratio, rnd }: HitInput): Rendered {
  const out = sweep(0.45, 130 * ratio, 55 * ratio, 0.035, 0.15, 0.6);
  noiseHit(out, rnd, 'low', 400, 0.7, 0.03, 0.34);
  return start(finish(out, 4));
}

export const EFFECTS = {
  tick,
  click,
  doubleclick,
  pop,
  ding,
  chime,
  sparkle,
  whoosh,
  revswell,
  roll,
  boing,
  thud,
} as const satisfies Record<string, (input: HitInput) => Rendered>;

export type EffectName = keyof typeof EFFECTS;

/** The reversed whoosh, `fx('whoosh', { reverse: true })`: it swells into its time. */
export const WHOOSH_INTO = whooshInto;

/** Effects whose `dur` sets their length. */
export const EFFECTS_WITH_DURATION: ReadonlySet<EffectName> = new Set<EffectName>(['sparkle', 'whoosh', 'revswell', 'roll']);
