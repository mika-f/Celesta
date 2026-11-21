// Deterministic math for the synthesizer.
//
// IEEE 754 rounds every +, -, *, / and sqrt to one exact result, and
// JavaScript neither fuses nor reorders them, so arithmetic built only from
// those gives the same bits on every engine and system. `Math.sin`, `exp`,
// `pow`, `tanh` and the rest are only required to be approximately right and
// differ between engines, so nothing here or in the voices calls them: these
// functions stand in for them, with an accuracy far beyond what 16-bit audio
// can show.

export const PI = 3.141592653589793;
export const TAU = 2 * PI;
export const HALF_PI = PI / 2;
export const LN2 = 0.6931471805599453;

const S3 = -1 / 6;
const S5 = 1 / 120;
const S7 = -1 / 5040;
const S9 = 1 / 362880;
const S11 = -1 / 39916800;
const S13 = 1 / 6227020800;
const S15 = -1 / 1307674368000;

/** sin(x), within about 1e-12 for |x| up to a few thousand. */
export function sin(x: number): number {
  let r = x - Math.round(x / TAU) * TAU;
  if (r > HALF_PI) r = PI - r;
  else if (r < -HALF_PI) r = -PI - r;
  const r2 = r * r;
  return r * (1 + r2 * (S3 + r2 * (S5 + r2 * (S7 + r2 * (S9 + r2 * (S11 + r2 * (S13 + r2 * S15)))))));
}

export function cos(x: number): number {
  return sin(x + HALF_PI);
}

/** sin(2π · phase) for a phase in cycles. */
export function sinCycles(phase: number): number {
  return sin(TAU * (phase - Math.floor(phase)));
}

/** 2^k for an integer k, exactly. */
function pow2(k: number): number {
  let result = 1;
  let base = k < 0 ? 0.5 : 2;
  let n = k < 0 ? -k : k;
  while (n > 0) {
    if (n & 1) result *= base;
    base *= base;
    n >>= 1;
  }
  return result;
}

/** e^x, to within a few units in the last place. */
export function exp(x: number): number {
  if (x !== x) return x;
  if (x < -745) return 0;
  if (x > 709) return Infinity;
  const k = Math.round(x / LN2);
  const r = x - k * LN2;
  let term = 1;
  let sum = 1;
  for (let n = 1; n <= 14; n += 1) {
    term = (term * r) / n;
    sum += term;
  }
  // 2^k may overflow for k near 1024 while the product would not; split it.
  if (k > 1000) return sum * pow2(k - 1000) * pow2(1000);
  if (k < -1000) return sum * pow2(k + 1000) * pow2(-1000);
  return sum * pow2(k);
}

/** 2^(semitones / 12): the frequency ratio of an interval. */
export function semitoneRatio(semitones: number): number {
  return exp((semitones * LN2) / 12);
}

/** The frequency of a MIDI note number (A4 = 69 = 440 Hz). */
export function midiToHz(midi: number): number {
  return 440 * semitoneRatio(midi - 69);
}

/** A smooth limiter: x / (1 + |x|) shaped to stay linear near zero. */
export function softClip(x: number, knee: number): number {
  const a = x < 0 ? -x : x;
  if (a <= knee) return x;
  const over = (a - knee) / (1 - knee);
  const shaped = knee + ((1 - knee) * over) / (1 + over);
  return x < 0 ? -shaped : shaped;
}

/** A 32-bit FNV-1a hash of a string. */
export function hashString(text: string, seed = 0x811c9dc5): number {
  let h = seed >>> 0;
  for (let i = 0; i < text.length; i += 1) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h;
}

/** Mulberry32: a seeded generator of numbers in [0, 1). */
export function random(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
